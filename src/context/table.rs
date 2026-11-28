use super::{ContextError, ContextRecord, lexical};
use crate::prelude::*;

use arrow_array::{Float32Array, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use futures::StreamExt;
use lancedb::{
    Connection, DistanceType, Table,
    database::CreateTableMode,
    index,
    query::{ExecutableQuery, QueryBase, Select},
    table::OptimizeAction,
};
use serde::de::DeserializeOwned;
use std::collections::HashMap;

/// Reciprocal rank fusion constant (the usual value).
const RRF_K: f32 = 60.0;

/// Below this number of rows an ANN index is pointless: exact (flat) search is fast and 100% accurate,
/// and IVF-PQ cannot even be trained on a handful of vectors.
const ANN_MIN_ROWS: usize = 4096;

/// The lexical scan reads all rows, so it is skipped on huge tables (use vector search only there).
const LEXICAL_MAX_ROWS: usize = 20_000;

/// Compaction is attempted once per this number of table versions (every write creates a version).
const OPTIMIZE_EVERY_VERSIONS: u64 = 16;

/// No automatic compaction on tables bigger than this (call `optimize` explicitly).
const AUTO_OPTIMIZE_MAX_ROWS: usize = 200_000;

/// Refine factor for PQ-compressed indexes (recovers the precision lost by quantization).
const REFINE_FACTOR: u32 = 5;

/// Search result with relevance details.
#[derive(Debug, Clone)]
pub struct ScoredRecord<T> {
    /// Record ID.
    pub id: u64,
    /// Record data.
    pub data: T,
    /// Final relevance in `0..=1` (1 = first both by meaning and by words).
    pub score: f32,
    /// Cosine similarity of the record to the query, if the record is among the nearest vectors.
    pub similarity: Option<f32>,
    /// Lexical match strength in `0..=1` (0 = no word matched).
    pub lexical: f32,
}

/// Options of the hybrid search.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum number of results.
    pub limit: usize,
    /// Minimum cosine similarity (`-1.0` = no limit). Lexical matches are admitted regardless.
    pub min_similarity: f32,
    /// Drop vector hits that are further than `margin` from the best one (cuts the noisy tail).
    pub margin: Option<f32>,
    /// Query text for the lexical channel (`None` = vector search only).
    pub text: Option<String>,
    /// Top-level JSON fields of the record used for the lexical channel (`None` = all strings).
    pub lexical_fields: Option<Vec<String>>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            min_similarity: 0.0,
            margin: None,
            text: None,
            lexical_fields: None,
        }
    }
}

/// A decoded table row.
struct Row {
    id: u64,
    data: String,
    /// Distance reported by the vector search (cosine distance = `1 - similarity`).
    distance: Option<f32>,
}

/// RAG database table (LanceDB).
#[derive(Clone)]
pub struct ContextTable {
    connection: Arc<Connection>,
    name: String,
}

impl ContextTable {
    /// Creates new table instance.
    pub(super) fn new(connection: Arc<Connection>, name: impl Into<String>) -> Self {
        Self {
            connection,
            name: name.into(),
        }
    }

    /// Opens the table if it exists.
    async fn open(&self) -> Option<Table> {
        self.connection.open_table(&self.name).execute().await.ok()
    }

    /// Searches by meaning (cosine similarity) and, optionally, by words, then fuses both rankings (RRF).
    ///
    /// * the vector channel uses the **cosine** metric explicitly (the LanceDB default is L2, which makes
    ///   thresholds depend on the vector length);
    /// * the lexical channel finds rare exact tokens that embeddings smear (see [`lexical`]);
    /// * results are ordered by the fused rank; `score` is the fused rank normalized to `0..=1`.
    pub async fn search<T>(
        &self,
        vector: Vec<f32>,
        options: SearchOptions,
    ) -> Result<Vec<ScoredRecord<T>>, Error>
    where
        T: DeserializeOwned,
    {
        let limit = options.limit.max(1);
        let Some(table) = self.open().await else {
            return Ok(vec![]);
        };

        // 1. vector channel (exact when no ANN index exists)
        let candidates = (limit * 4).max(20);
        let vector_rows = self.vector_candidates(&table, &vector, candidates).await?;

        // similarities of all candidates (also used to report the similarity of lexical-only hits)
        let mut all_sims: HashMap<u64, f32> = HashMap::new();
        let mut vector_hits: Vec<(Row, f32)> = Vec::new();
        for row in vector_rows {
            let Some(distance) = row.distance else {
                continue;
            };
            let sim = 1.0 - distance;
            all_sims.insert(row.id, sim);
            vector_hits.push((row, sim));
        }

        // keep hits above the floor and close enough to the best one
        let best = vector_hits.first().map(|(_, s)| *s).unwrap_or(-1.0);
        vector_hits.retain(|(_, sim)| {
            *sim >= options.min_similarity && options.margin.map_or(true, |m| *sim >= best - m)
        });

        // 2. lexical channel (failures never break the vector search)
        let text = options
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let lexical_hits = match text {
            Some(text) => self
                .lexical_scan(&table, text, options.lexical_fields.as_deref())
                .await
                .unwrap_or_default(),
            None => vec![],
        };
        // the score is normalized by the channels that actually produced results
        let channels = if lexical_hits.is_empty() { 1.0 } else { 2.0 };

        // 3. reciprocal rank fusion
        struct Acc {
            data: String,
            lexical: f32,
            fused: f32,
        }
        let mut acc: HashMap<u64, Acc> = HashMap::new();

        for (rank, (row, _)) in vector_hits.into_iter().enumerate() {
            acc.insert(
                row.id,
                Acc {
                    data: row.data,
                    lexical: 0.0,
                    fused: 1.0 / (RRF_K + rank as f32 + 1.0),
                },
            );
        }
        for (rank, (row, lex)) in lexical_hits.into_iter().enumerate() {
            let part = 1.0 / (RRF_K + rank as f32 + 1.0);
            let entry = acc.entry(row.id).or_insert(Acc {
                data: row.data,
                lexical: 0.0,
                fused: 0.0,
            });
            entry.lexical = lex;
            entry.fused += part;
        }

        let mut merged: Vec<(u64, Acc)> = acc.into_iter().collect();
        merged.sort_by(|(ida, a), (idb, b)| {
            b.fused
                .partial_cmp(&a.fused)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let sa = all_sims.get(ida).copied().unwrap_or(-1.0);
                    let sb = all_sims.get(idb).copied().unwrap_or(-1.0);
                    sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        merged.truncate(limit);

        let max_fused = channels / (RRF_K + 1.0);
        let mut results = Vec::with_capacity(merged.len());
        for (id, a) in merged {
            results.push(ScoredRecord {
                id,
                data: json::from_str(&a.data)?,
                score: (a.fused / max_fused).clamp(0.0, 1.0),
                similarity: all_sims.get(&id).copied(),
                lexical: a.lexical,
            });
        }

        Ok(results)
    }

    /// Reads similar data by embeddings vector (cosine similarity `>= coef`, best first).
    ///
    /// Kept for compatibility, `search` gives more control.
    pub async fn read<T>(
        &self,
        vector: Vec<f32>,
        limit: Option<usize>,
        coef: f32,
    ) -> Result<Option<Vec<ContextRecord<T>>>, Error>
    where
        T: DeserializeOwned,
    {
        let found = self
            .search::<T>(
                vector,
                SearchOptions {
                    limit: limit.unwrap_or(10),
                    min_similarity: coef,
                    ..Default::default()
                },
            )
            .await?;

        if found.is_empty() {
            return Ok(None);
        }

        Ok(Some(
            found
                .into_iter()
                .map(|r| ContextRecord {
                    id: r.id,
                    data: r.data,
                })
                .collect(),
        ))
    }

    /// Nearest vectors by cosine distance, best first.
    async fn vector_candidates(
        &self,
        table: &Table,
        vector: &[f32],
        count: usize,
    ) -> Result<Vec<Row>, Error> {
        // the refine factor is meaningful only for PQ indexes
        let indexed = table
            .list_indices()
            .await
            .map(|list| !list.is_empty())
            .unwrap_or(false);

        let mut query = table
            .query()
            .nearest_to(vector)
            .map_err(ContextError::from)?
            .distance_type(DistanceType::Cosine);

        if indexed {
            query = query.refine_factor(REFINE_FACTOR);
        }

        let mut stream = query
            .limit(count)
            .execute()
            .await
            .map_err(ContextError::from)?;

        let mut rows = Vec::new();
        while let Some(batch) = stream.next().await {
            let batch = batch.map_err(ContextError::from)?;
            rows.extend(decode_batch(&batch)?);
        }

        rows.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(rows)
    }

    /// Lexical scan of all rows (vectors are not read). Returns matching rows with their lexical score.
    async fn lexical_scan(
        &self,
        table: &Table,
        text: &str,
        fields: Option<&[String]>,
    ) -> Result<Vec<(Row, f32)>, Error> {
        let total = table.count_rows(None).await.map_err(ContextError::from)?;
        if total == 0 || total > LEXICAL_MAX_ROWS {
            return Ok(vec![]);
        }

        let mut stream = table
            .query()
            .select(Select::columns(&["id", "data"]))
            .execute()
            .await
            .map_err(ContextError::from)?;

        let mut rows = Vec::with_capacity(total);
        while let Some(batch) = stream.next().await {
            let batch = batch.map_err(ContextError::from)?;
            rows.extend(decode_batch(&batch)?);
        }

        let docs: Vec<Vec<String>> = rows
            .iter()
            .map(|r| lexical::words(&searchable_text(&r.data, fields)))
            .collect();

        let scores: HashMap<usize, f32> = lexical::score(text, &docs).into_iter().collect();
        let mut hits: Vec<(Row, f32)> = rows
            .into_iter()
            .enumerate()
            .filter_map(|(i, row)| scores.get(&i).map(|s| (row, *s)))
            .collect();

        hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(hits)
    }

    /// Reads all records from the table.
    pub async fn read_all<T>(&self) -> Result<Option<Vec<ContextRecord<T>>>, Error>
    where
        T: DeserializeOwned,
    {
        let Some(table) = self.open().await else {
            return Ok(None);
        };

        // perform a full scan of the table (vectors are not needed)
        let mut stream = table
            .query()
            .select(Select::columns(&["id", "data"]))
            .execute()
            .await
            .map_err(ContextError::from)?;

        let mut results = Vec::new();
        while let Some(batch) = stream.next().await {
            let batch = batch.map_err(ContextError::from)?;
            for row in decode_batch(&batch)? {
                let data: T = json::from_str(&row.data)?;
                results.push(ContextRecord { id: row.id, data });
            }
        }

        if results.is_empty() {
            return Ok(None);
        }

        Ok(Some(results))
    }

    /// Writes record to the table.
    pub async fn write<T>(&self, id: u64, vector: Vec<f32>, data: T) -> Result<(), Error>
    where
        T: serde::Serialize,
    {
        self.write_batch(vec![(id, vector, data)]).await
    }

    /// Writes batch of records (u64, Vec<f32>, T).
    pub async fn write_batch<T>(&self, batch_data: Vec<(u64, Vec<f32>, T)>) -> Result<(), Error>
    where
        T: serde::Serialize,
    {
        if batch_data.is_empty() {
            return Ok(());
        }

        let (batch, dim) = build_batch(batch_data)?;
        let batches = vec![batch];

        match self.open().await {
            Some(table) => {
                if let Some(existing) = vector_dim(&table).await {
                    if existing != dim {
                        // an empty table may be recreated with the new embeddings dimension
                        let rows = table.count_rows(None).await.map_err(ContextError::from)?;
                        if rows > 0 {
                            return Err(dimension_error(existing, dim));
                        }

                        self.connection
                            .create_table(&self.name, batches)
                            .mode(CreateTableMode::Overwrite)
                            .execute()
                            .await
                            .map_err(ContextError::from)?;
                        return Ok(());
                    }
                }

                table
                    .add(batches)
                    .execute()
                    .await
                    .map_err(ContextError::from)?;
            }
            None => {
                self.connection
                    .create_table(&self.name, batches)
                    .mode(CreateTableMode::Create)
                    .execute()
                    .await
                    .map_err(ContextError::from)?;
            }
        };

        self.auto_optimize().await;
        Ok(())
    }

    /// Atomically replaces ALL records with the given ones (the new table version appears at once).
    ///
    /// The right tool for re-embedding after switching the embeddings model: the vector dimension may change.
    pub async fn replace_all<T>(&self, batch_data: Vec<(u64, Vec<f32>, T)>) -> Result<(), Error>
    where
        T: serde::Serialize,
    {
        if batch_data.is_empty() {
            return self.clear().await;
        }

        let (batch, _) = build_batch(batch_data)?;

        self.connection
            .create_table(&self.name, vec![batch])
            .mode(CreateTableMode::Overwrite)
            .execute()
            .await
            .map_err(ContextError::from)?;

        Ok(())
    }

    /// Removes record by ID.
    pub async fn remove(&self, id: u64) -> Result<(), Error> {
        if let Some(table) = self.open().await {
            let predicate = format!("id = {}", id);
            table.delete(&predicate).await.map_err(ContextError::from)?;
        }

        self.auto_optimize().await;
        Ok(())
    }

    /// Clears all records in the table.
    pub async fn clear(&self) -> Result<(), Error> {
        if let Some(table) = self.open().await {
            table.delete("1 = 1").await.map_err(ContextError::from)?;
        }

        Ok(())
    }

    /// Compacts small data files, drops old versions and indexes new rows.
    ///
    /// Every single-record write creates a data file and a table version; without compaction
    /// reads slow down over time. Done automatically (see `auto_optimize`) for small tables.
    pub async fn optimize(&self) -> Result<(), Error> {
        if let Some(table) = self.open().await {
            table
                .optimize(OptimizeAction::All)
                .await
                .map_err(ContextError::from)?;
        }

        Ok(())
    }

    /// Cheap stateless compaction policy: once per `OPTIMIZE_EVERY_VERSIONS` versions, small tables only.
    async fn auto_optimize(&self) {
        let Some(table) = self.open().await else {
            return;
        };

        let Ok(version) = table.version().await else {
            return;
        };
        if version == 0 || version % OPTIMIZE_EVERY_VERSIONS != 0 {
            return;
        }

        if matches!(table.count_rows(None).await, Ok(rows) if rows <= AUTO_OPTIMIZE_MAX_ROWS) {
            let _ = table.optimize(OptimizeAction::All).await;
        }
    }

    /// Builds an ANN index only when it pays off (see `ANN_MIN_ROWS`); otherwise exact search is used.
    ///
    /// Parameters follow the table size: `sqrt(rows)` partitions and 8+ dimensions per PQ sub-vector
    /// (the sub-vector count always divides the dimension). An existing index is kept:
    /// `optimize` appends new rows to it.
    pub async fn ensure_index(&self) -> Result<(), Error> {
        let Some(table) = self.open().await else {
            return Ok(());
        };

        let rows = table.count_rows(None).await.map_err(ContextError::from)?;
        if rows < ANN_MIN_ROWS {
            return Ok(());
        }

        let has_index = table
            .list_indices()
            .await
            .map(|list| !list.is_empty())
            .unwrap_or(false);
        if has_index {
            return Ok(());
        }

        let Some(dim) = vector_dim(&table).await else {
            return Ok(());
        };

        let partitions = ((rows as f64).sqrt() as u32).clamp(2, 1024);
        self.index(partitions, pq_subvectors(dim)).await
    }

    /// Builds (or rebuilds) the IVF-PQ index with explicit parameters (cosine metric).
    pub async fn index(&self, partitions: u32, subvectors: u32) -> Result<(), Error> {
        if let Some(table) = self.open().await {
            let index_config = index::Index::IvfPq(
                index::vector::IvfPqIndexBuilder::default()
                    .distance_type(DistanceType::Cosine)
                    .num_partitions(partitions)
                    .num_sub_vectors(subvectors),
            );

            if let Err(e) = table
                .create_index(&["vector"], index_config)
                .replace(true)
                .execute()
                .await
                .map_err(ContextError::from)
            {
                let err_msg = e.to_string();
                if err_msg.contains("KMeans cannot train") || err_msg.contains("choose a smaller K")
                {
                    return Ok(());
                }
                return Err(e.into());
            }
        }

        Ok(())
    }
}

/// Decodes `id`, `data` and (if present) `_distance` columns of a batch.
fn decode_batch(batch: &RecordBatch) -> Result<Vec<Row>, ContextError> {
    let id_col = batch
        .column_by_name("id")
        .ok_or_else(|| ContextError::ExpectedColumn("id"))?
        .as_any()
        .downcast_ref::<UInt64Array>()
        .ok_or_else(|| ContextError::FailedDowncast("id", "UInt64Array"))?;

    let data_col = batch
        .column_by_name("data")
        .ok_or_else(|| ContextError::ExpectedColumn("data"))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ContextError::FailedDowncast("data", "StringArray"))?;

    let distance_col = batch
        .column_by_name("_distance")
        .and_then(|col| col.as_any().downcast_ref::<Float32Array>());

    Ok((0..batch.num_rows())
        .map(|i| Row {
            id: id_col.value(i),
            data: data_col.value(i).to_owned(),
            distance: distance_col.map(|d| d.value(i)),
        })
        .collect())
}

/// Builds one record batch from `(id, vector, data)` items. Returns the batch and the vector dimension.
fn build_batch<T>(items: Vec<(u64, Vec<f32>, T)>) -> Result<(RecordBatch, usize), Error>
where
    T: serde::Serialize,
{
    let count = items.len();
    let dim = items[0].1.len();

    let mut ids = Vec::with_capacity(count);
    let mut flat = Vec::with_capacity(count * dim);
    let mut jsons = Vec::with_capacity(count);

    for (id, mut vector, data) in items {
        if vector.len() != dim {
            return Err(ContextError::InvalidBatchLength.into());
        }
        ids.push(id);
        flat.append(&mut vector);
        jsons.push(json::to_string(&data)?);
    }

    let item_field = Arc::new(Field::new("item", DataType::Float32, true));
    let vector_array = Arc::new(
        arrow_array::FixedSizeListArray::try_new(
            item_field.clone(),
            dim as i32,
            Arc::new(Float32Array::from(flat)) as Arc<dyn arrow_array::Array>,
            None,
        )
        .map_err(ContextError::from)?,
    );

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(item_field, dim as i32),
            false,
        ),
        Field::new("data", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(UInt64Array::from(ids)) as Arc<dyn arrow_array::Array>,
            vector_array as Arc<dyn arrow_array::Array>,
            Arc::new(StringArray::from(jsons)) as Arc<dyn arrow_array::Array>,
        ],
    )
    .map_err(ContextError::from)?;

    Ok((batch, dim))
}

/// Dimension of the `vector` column of an existing table.
async fn vector_dim(table: &Table) -> Option<usize> {
    let schema = table.schema().await.ok()?;
    let field = schema.field_with_name("vector").ok()?;

    match field.data_type() {
        DataType::FixedSizeList(_, dim) => Some(*dim as usize),
        _ => None,
    }
}

/// Number of PQ sub-vectors: the largest divisor of `dim` that keeps at least 8 dimensions per sub-vector.
fn pq_subvectors(dim: usize) -> u32 {
    (1..=dim / 8).rev().find(|d| dim % d == 0).unwrap_or(1) as u32
}

fn dimension_error(existing: usize, actual: usize) -> Error {
    ContextError::from(lancedb::Error::InvalidInput {
        message: format!(
            "Embedding dimension mismatch: the table holds {existing}-dimensional vectors, got {actual}. \
             Re-embed the data (replace_all) after changing the embeddings model."
        ),
    })
    .into()
}

/// Text of a record for the lexical channel: the chosen top-level fields, or all strings.
fn searchable_text(raw: &str, fields: Option<&[String]>) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_owned();
    };

    let mut out = String::new();
    match (fields, &value) {
        (Some(fields), serde_json::Value::Object(map)) => {
            for field in fields {
                if let Some(v) = map.get(field) {
                    collect_strings(v, &mut out);
                }
            }
        }
        _ => collect_strings(&value, &mut out),
    }

    out
}

fn collect_strings(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::String(s) => {
            out.push_str(s);
            out.push(' ');
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}
