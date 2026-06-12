use super::RagRecord;
use crate::prelude::*;

use arrow_array::{Float32Array, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use lancedb::{
    Connection,
    database::CreateTableMode,
    index,
    query::{ExecutableQuery, QueryBase},
};
use serde::de::DeserializeOwned;

/// The RAG database table (LanceDB)
#[derive(Clone)]
pub struct RagTable {
    connection: Arc<Connection>,
    name: String,
}

impl RagTable {
    /// Creates a new table instance
    pub(super) fn new(connection: Arc<Connection>, name: impl Into<String>) -> Self {
        Self {
            connection,
            name: name.into(),
        }
    }

    /// Reads a similar data by embeddings vector
    pub async fn read<T>(
        &self,
        vector: Vec<f32>,
        limit: usize,
        coef: f32,
    ) -> Result<Option<Vec<RagRecord<T>>>>
    where
        T: DeserializeOwned,
    {
        let table = match self.connection.open_table(&self.name).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let max_distance = (1.0f32 - coef).max(0.0f32);

        // vector search using LanceDB tools:
        let mut stream = table
            .query()
            .nearest_to(vector.as_slice())?
            .limit(limit)
            .execute()
            .await?;

        let mut results = Vec::new();

        while let Some(batch_result) = stream.next().await {
            let batch = batch_result?;

            // safely remove columns by name:
            let id_col = batch
                .column_by_name("id")
                .ok_or_else(|| Error::ExpectedColumn("id"))?
                .as_any()
                .downcast_ref::<UInt64Array>()
                .ok_or_else(|| Error::FailedDowncast("id", "UInt64Array"))?;

            let data_col = batch
                .column_by_name("data")
                .ok_or_else(|| Error::ExpectedColumn("column"))?
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| Error::FailedDowncast("data", "StringArray"))?;

            let distance_col = batch
                .column_by_name("_distance")
                .map(|col| col.as_any().downcast_ref::<Float32Array>())
                .flatten();

            for i in 0..batch.num_rows() {
                // filtering by distance (similarity coefficient):
                if let Some(dist_arr) = distance_col {
                    if dist_arr.value(i) > max_distance {
                        continue;
                    }
                }

                let id = id_col.value(i);
                let json_str = data_col.value(i);
                let data: T = json::from_str(json_str)?;

                results.push(RagRecord { id, data });
            }
        }

        if results.is_empty() {
            return Ok(None);
        }

        Ok(Some(results))
    }

    /// Writes any serializable data to the table
    pub async fn write<T>(&self, vector: Vec<f32>, data: T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let vector_len = vector.len();

        // generating unique id:
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let rand_part = (rand::random::<u32>() & 0x3F_FFFF) as u64;
        let id = (now_ms << 22) | rand_part;

        // preparing arrow-arrays:
        let id_array = Arc::new(UInt64Array::from(vec![id]));

        let float_array = Arc::new(Float32Array::from(vector));
        let item_field = Arc::new(Field::new("item", DataType::Float32, true));
        let vector_array = Arc::new(arrow_array::FixedSizeListArray::try_new(
            item_field,
            vector_len as i32,
            float_array as Arc<dyn arrow_array::Array>,
            None,
        )?);

        let json_string = serde_json::to_string(&data)?;
        let data_array = Arc::new(StringArray::from(vec![json_string]));

        // describing the scheme:
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::UInt64, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    vector_len as i32,
                ),
                false,
            ),
            Field::new("data", DataType::Utf8, false),
        ]));

        // building the record batch:
        let batch = RecordBatch::try_new(
            schema,
            vec![
                id_array as Arc<dyn arrow_array::Array>,
                vector_array as Arc<dyn arrow_array::Array>,
                data_array as Arc<dyn arrow_array::Array>,
            ],
        )?;

        let batches = vec![batch];

        // writing to table:
        match self.connection.open_table(&self.name).execute().await {
            Ok(table) => {
                table.add(batches).execute().await?;
            }
            Err(_) => {
                self.connection
                    .create_table(&self.name, batches)
                    .mode(CreateTableMode::Create)
                    .execute()
                    .await?;
            }
        };

        Ok(())
    }

    /// Writes a batch of serializable data
    pub async fn write_batch<T>(&self, batch_data: Vec<(Vec<f32>, T)>) -> Result<()>
    where
        T: serde::Serialize,
    {
        if batch_data.is_empty() {
            return Ok(());
        }

        let batch_size = batch_data.len();
        let vector_len = batch_data[0].0.len();

        // generate unique ids:
        let mut ids = Vec::with_capacity(batch_size);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for i in 0..batch_size {
            let rand_part = (rand::random::<u32>() & 0x3F_FFFF) as u64;
            let id = ((now_ms + i as u64) << 22) | rand_part;
            ids.push(id);
        }
        let id_array = Arc::new(UInt64Array::from(ids));

        // allocating memory for vectors:
        let mut flat_vectors = Vec::with_capacity(batch_size * vector_len);
        let mut json_strings = Vec::with_capacity(batch_size);

        for (mut vector, data) in batch_data {
            if vector.len() != vector_len {
                return Err(Error::InvalidBatchLength.into());
            }
            flat_vectors.append(&mut vector);
            json_strings.push(json::to_string(&data)?);
        }

        // creating arrow-arrays:
        let float_array = Arc::new(Float32Array::from(flat_vectors));
        let item_field = Arc::new(Field::new("item", DataType::Float32, true));
        let vector_array = Arc::new(arrow_array::FixedSizeListArray::try_new(
            item_field,
            vector_len as i32,
            float_array as Arc<dyn arrow_array::Array>,
            None,
        )?);

        let data_array = Arc::new(StringArray::from(json_strings));

        // describing the scheme:
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::UInt64, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    vector_len as i32,
                ),
                false,
            ),
            Field::new("data", DataType::Utf8, false),
        ]));

        // building the record batch:
        let record_batch = RecordBatch::try_new(
            schema,
            vec![
                id_array as Arc<dyn arrow_array::Array>,
                vector_array as Arc<dyn arrow_array::Array>,
                data_array as Arc<dyn arrow_array::Array>,
            ],
        )?;

        let batches = vec![record_batch];

        // writing to table:
        match self.connection.open_table(&self.name).execute().await {
            Ok(table) => {
                table.add(batches).execute().await?;
            }
            Err(_) => {
                self.connection
                    .create_table(&self.name, batches)
                    .mode(CreateTableMode::Create)
                    .execute()
                    .await?;
            }
        };

        Ok(())
    }

    /// Removes a table record by ID
    pub async fn remove(&self, id: u64) -> Result<()> {
        if let Ok(table) = self.connection.open_table(&self.name).execute().await {
            let predicate = str!("id = {}", id);
            table.delete(&predicate).await?;
        }

        Ok(())
    }

    // Optimizes the table indexing
    pub async fn index(&self, partitions: u32, subvectors: u32) -> Result<()> {
        if let Ok(table) = self.connection.open_table(&self.name).execute().await {
            let index_config = index::Index::IvfPq(
                index::vector::IvfPqIndexBuilder::default()
                    .num_partitions(partitions)
                    .num_sub_vectors(subvectors),
            );

            if let Err(e) = table
                .create_index(&["vector"], index_config)
                .execute()
                .await
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
