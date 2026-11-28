//! Integration tests of the RAG context database (`Context` / `ContextTable`).
//!
//! The `lexical` module is private, so it is tested through the public hybrid search
//! (`SearchOptions::text` / `lexical_fields`). Vectors are tiny one-hot-like vectors,
//! so cosine similarities are known exactly.
//!
//! NOTE: adjust the crate/module path in the `use` below to match your project.

use cistern::context::{self, Context, ContextRecord, SearchOptions};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

/// Temporary directory removed on drop (no extra dev-dependency needed).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "context-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Fact {
    text: String,
}

fn fact(text: &str) -> Fact {
    Fact { text: text.into() }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Note {
    title: String,
    body: String,
}

fn note(title: &str, body: &str) -> Note {
    Note {
        title: title.into(),
        body: body.into(),
    }
}

/// One-hot vector of the given dimension.
fn axis(dim: usize, i: usize) -> Vec<f32> {
    let mut v = vec![0.0; dim];
    v[i] = 1.0;
    v
}

/// Opens a fresh database in a temp dir.
async fn setup() -> (TempDir, Context) {
    let dir = TempDir::new();
    let ctx = Context::connect(dir.path()).await.unwrap();
    (dir, ctx)
}

const DIM: usize = 8;
const BASE_ID: u64 = 100;

/// Corpus for lexical tests: record `i` has id `BASE_ID + i` and vector `axis(DIM, i)`.
const CORPUS: &[&str] = &[
    "Пользователь использует Neovim как основной редактор",
    "Пользователь живёт в Хельсинки",
    "У пользователя кот по кличке Том",
    "Пользователь пишет на Rust, проект называется osy",
    "Видеокарта пользователя RTX 4070",
    "Пользователь предпочитает тёмную тему",
    "User's favorite coffee is flat white",
    "Пользователь любит кофе и пьёт его без сахара",
];

async fn corpus_table(ctx: &Context) -> context::ContextTable {
    let table = ctx.open_table("facts").await.unwrap();
    let batch: Vec<_> = CORPUS
        .iter()
        .enumerate()
        .map(|(i, t)| (BASE_ID + i as u64, axis(DIM, i), fact(t)))
        .collect();
    table.write_batch(batch).await.unwrap();
    table
}

fn ids<T>(found: &[context::ScoredRecord<T>]) -> Vec<u64> {
    found.iter().map(|r| r.id).collect()
}

fn lexical_opts(text: &str) -> SearchOptions {
    SearchOptions {
        // admit only the exact vector match, everything else must come from the lexical channel
        min_similarity: 0.99,
        text: Some(text.into()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------
// missing / empty tables
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn missing_table_reads_as_empty() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("nothing").await.unwrap();

    assert!(table.read_all::<Fact>().await.unwrap().is_none());
    assert!(
        table
            .read::<Fact>(axis(DIM, 0), None, 0.0)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        table
            .search::<Fact>(axis(DIM, 0), SearchOptions::default())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn missing_table_mutations_are_noops() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("nothing").await.unwrap();

    table.remove(1).await.unwrap();
    table.clear().await.unwrap();
    table.optimize().await.unwrap();
    table.ensure_index().await.unwrap();
    table.index(2, 1).await.unwrap();

    assert!(table.read_all::<Fact>().await.unwrap().is_none());
}

#[tokio::test]
async fn empty_batch_does_not_create_table() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();

    table.write_batch::<Fact>(vec![]).await.unwrap();

    assert!(table.read_all::<Fact>().await.unwrap().is_none());
}

// ---------------------------------------------------------------------------------------------
// write / read_all / remove / clear
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn write_and_read_all_roundtrip() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();

    table.write(1, axis(4, 0), fact("первый")).await.unwrap();
    table.write(2, axis(4, 1), fact("second")).await.unwrap();

    let mut all = table.read_all::<Fact>().await.unwrap().unwrap();
    all.sort_by_key(|r| r.id);

    assert_eq!(all.len(), 2);
    assert_eq!((all[0].id, &all[0].data), (1, &fact("первый")));
    assert_eq!((all[1].id, &all[1].data), (2, &fact("second")));
}

#[tokio::test]
async fn write_batch_roundtrip() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();

    let batch: Vec<_> = (0..5u64)
        .map(|i| (i, axis(5, i as usize), fact(&format!("fact {i}"))))
        .collect();
    table.write_batch(batch).await.unwrap();

    let mut all = table.read_all::<Fact>().await.unwrap().unwrap();
    all.sort_by_key(|r| r.id);

    assert_eq!(all.len(), 5);
    for (i, rec) in all.iter().enumerate() {
        assert_eq!(rec.id, i as u64);
        assert_eq!(rec.data, fact(&format!("fact {i}")));
    }
}

#[tokio::test]
async fn write_batch_rejects_vectors_of_different_length() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();

    let batch = vec![
        (1, vec![1.0, 0.0, 0.0], fact("a")),
        (2, vec![1.0, 0.0], fact("b")),
    ];

    assert!(table.write_batch(batch).await.is_err());
    // nothing was written
    assert!(table.read_all::<Fact>().await.unwrap().is_none());
}

#[tokio::test]
async fn read_all_fails_on_incompatible_type() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table.write(1, axis(4, 0), fact("text")).await.unwrap();

    // `Note` requires `title` and `body`, the stored JSON has only `text`
    assert!(table.read_all::<Note>().await.is_err());
}

#[tokio::test]
async fn remove_deletes_only_the_given_record() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table
        .write_batch(vec![
            (1, axis(4, 0), fact("one")),
            (2, axis(4, 1), fact("two")),
            (3, axis(4, 2), fact("three")),
        ])
        .await
        .unwrap();

    table.remove(2).await.unwrap();

    let mut left: Vec<u64> = table
        .read_all::<Fact>()
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    left.sort();
    assert_eq!(left, vec![1, 3]);

    // removed record is not searchable either
    let found = table
        .search::<Fact>(axis(4, 1), SearchOptions::default())
        .await
        .unwrap();
    assert!(!ids(&found).contains(&2));
}

#[tokio::test]
async fn remove_unknown_id_is_ok() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table.write(1, axis(4, 0), fact("one")).await.unwrap();

    table.remove(999).await.unwrap();

    assert_eq!(table.read_all::<Fact>().await.unwrap().unwrap().len(), 1);
}

#[tokio::test]
async fn clear_removes_all_records() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table
        .write_batch(vec![(1, axis(4, 0), fact("a")), (2, axis(4, 1), fact("b"))])
        .await
        .unwrap();

    table.clear().await.unwrap();

    assert!(table.read_all::<Fact>().await.unwrap().is_none());
    assert!(
        table
            .search::<Fact>(axis(4, 0), SearchOptions::default())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn many_single_writes_survive_auto_optimize() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();

    // more than OPTIMIZE_EVERY_VERSIONS (16) versions: compaction must not lose anything
    for i in 0..40u64 {
        table
            .write(i, axis(4, (i % 4) as usize), fact(&format!("fact {i}")))
            .await
            .unwrap();
    }

    let mut all: Vec<u64> = table
        .read_all::<Fact>()
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    all.sort();
    assert_eq!(all, (0..40).collect::<Vec<_>>());
}

#[tokio::test]
async fn explicit_optimize_keeps_data() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    for i in 0..5u64 {
        table
            .write(i, axis(5, i as usize), fact(&format!("fact {i}")))
            .await
            .unwrap();
    }

    table.optimize().await.unwrap();

    assert_eq!(table.read_all::<Fact>().await.unwrap().unwrap().len(), 5);
}

// ---------------------------------------------------------------------------------------------
// embeddings dimension
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn dimension_mismatch_is_rejected_for_non_empty_table() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table.write(1, axis(4, 0), fact("four")).await.unwrap();

    let err = table.write(2, axis(6, 0), fact("six")).await.unwrap_err();

    assert!(format!("{err:?}{err}").contains("dimension"));
    // old data is intact
    let all = table.read_all::<Fact>().await.unwrap().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, 1);
}

#[tokio::test]
async fn empty_table_accepts_new_dimension() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table.write(1, axis(4, 0), fact("four")).await.unwrap();
    table.clear().await.unwrap();

    table.write(2, axis(6, 5), fact("six")).await.unwrap();

    let found = table
        .search::<Fact>(axis(6, 5), SearchOptions::default())
        .await
        .unwrap();
    assert_eq!(ids(&found), vec![2]);
}

#[tokio::test]
async fn replace_all_can_change_dimension() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table
        .write_batch(vec![
            (1, axis(4, 0), fact("old")),
            (2, axis(4, 1), fact("old")),
        ])
        .await
        .unwrap();

    table
        .replace_all(vec![
            (10, axis(6, 0), fact("new")),
            (11, axis(6, 1), fact("new")),
        ])
        .await
        .unwrap();

    let mut all: Vec<u64> = table
        .read_all::<Fact>()
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    all.sort();
    assert_eq!(all, vec![10, 11]);

    let found = table
        .search::<Fact>(axis(6, 1), SearchOptions::default())
        .await
        .unwrap();
    assert_eq!(found[0].id, 11);
}

#[tokio::test]
async fn replace_all_on_missing_table_creates_it() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();

    table
        .replace_all(vec![(1, axis(4, 0), fact("a"))])
        .await
        .unwrap();

    assert_eq!(table.read_all::<Fact>().await.unwrap().unwrap().len(), 1);
}

#[tokio::test]
async fn replace_all_with_empty_batch_clears_table() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table.write(1, axis(4, 0), fact("a")).await.unwrap();

    table.replace_all::<Fact>(vec![]).await.unwrap();

    assert!(table.read_all::<Fact>().await.unwrap().is_none());
}

#[tokio::test]
async fn replace_all_rejects_inconsistent_batch_and_keeps_old_data() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table.write(1, axis(4, 0), fact("old")).await.unwrap();

    let result = table
        .replace_all(vec![
            (2, vec![1.0, 0.0], fact("a")),
            (3, vec![1.0], fact("b")),
        ])
        .await;

    assert!(result.is_err());
    let all = table.read_all::<Fact>().await.unwrap().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, 1);
}

// ---------------------------------------------------------------------------------------------
// vector search
// ---------------------------------------------------------------------------------------------

/// ids: 1 = exact match, 2 = close, 3 = orthogonal, 4 = opposite.
async fn vector_table(ctx: &Context) -> context::ContextTable {
    let table = ctx.open_table("vectors").await.unwrap();
    table
        .write_batch(vec![
            (1, vec![1.0, 0.0, 0.0, 0.0], fact("exact")),
            (2, vec![0.9, 0.1, 0.0, 0.0], fact("close")),
            (3, vec![0.0, 1.0, 0.0, 0.0], fact("orthogonal")),
            (4, vec![-1.0, 0.0, 0.0, 0.0], fact("opposite")),
        ])
        .await
        .unwrap();
    table
}

#[tokio::test]
async fn search_orders_by_cosine_similarity() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .search::<Fact>(
            vec![1.0, 0.0, 0.0, 0.0],
            SearchOptions {
                min_similarity: -1.0,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![1, 2, 3, 4]);

    // cosine similarity does not depend on the vector length
    assert!((found[0].similarity.unwrap() - 1.0).abs() < 1e-4);
    assert!(found[1].similarity.unwrap() > 0.98);
    assert!(found[2].similarity.unwrap().abs() < 1e-4);
    assert!((found[3].similarity.unwrap() + 1.0).abs() < 1e-4);

    // no lexical channel -> no lexical score
    assert!(found.iter().all(|r| r.lexical == 0.0));
    // payload is decoded
    assert_eq!(found[0].data, fact("exact"));
}

#[tokio::test]
async fn search_is_independent_of_query_length() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let short = table
        .search::<Fact>(vec![1.0, 0.0, 0.0, 0.0], SearchOptions::default())
        .await
        .unwrap();
    let long = table
        .search::<Fact>(vec![50.0, 0.0, 0.0, 0.0], SearchOptions::default())
        .await
        .unwrap();

    assert_eq!(ids(&short), ids(&long));
    assert!((short[0].similarity.unwrap() - long[0].similarity.unwrap()).abs() < 1e-4);
}

#[tokio::test]
async fn search_best_hit_has_score_one_and_scores_decrease() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .search::<Fact>(vec![1.0, 0.0, 0.0, 0.0], SearchOptions::default())
        .await
        .unwrap();

    assert!(!found.is_empty());
    assert!((found[0].score - 1.0).abs() < 1e-4);
    for pair in found.windows(2) {
        assert!(pair[0].score >= pair[1].score);
    }
    for r in &found {
        assert!(r.score > 0.0 && r.score <= 1.0);
    }
}

#[tokio::test]
async fn search_respects_limit() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .search::<Fact>(
            vec![1.0, 0.0, 0.0, 0.0],
            SearchOptions {
                limit: 2,
                min_similarity: -1.0,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![1, 2]);
}

#[tokio::test]
async fn search_limit_zero_is_treated_as_one() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .search::<Fact>(
            vec![1.0, 0.0, 0.0, 0.0],
            SearchOptions {
                limit: 0,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![1]);
}

#[tokio::test]
async fn search_min_similarity_filters_weak_hits() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .search::<Fact>(
            vec![1.0, 0.0, 0.0, 0.0],
            SearchOptions {
                min_similarity: 0.5,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![1, 2]);
}

#[tokio::test]
async fn search_margin_cuts_the_tail() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .search::<Fact>(
            vec![1.0, 0.0, 0.0, 0.0],
            SearchOptions {
                min_similarity: -1.0,
                margin: Some(0.1),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // only records within 0.1 of the best similarity survive
    assert_eq!(ids(&found), vec![1, 2]);
}

#[tokio::test]
async fn search_without_margin_keeps_everything_above_floor() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .search::<Fact>(
            vec![1.0, 0.0, 0.0, 0.0],
            SearchOptions {
                min_similarity: -1.0,
                margin: None,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(found.len(), 4);
}

// ---------------------------------------------------------------------------------------------
// `read` (compatibility API)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn read_returns_records_above_coef_best_first() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found: Vec<ContextRecord<Fact>> = table
        .read(vec![1.0, 0.0, 0.0, 0.0], None, 0.5)
        .await
        .unwrap()
        .unwrap();

    let got: Vec<u64> = found.iter().map(|r| r.id).collect();
    assert_eq!(got, vec![1, 2]);
    assert_eq!(found[0].data, fact("exact"));
}

#[tokio::test]
async fn read_respects_limit() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    let found = table
        .read::<Fact>(vec![1.0, 0.0, 0.0, 0.0], Some(1), -1.0)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, 1);
}

#[tokio::test]
async fn read_returns_none_when_nothing_is_similar_enough() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    // the query is orthogonal to the "exact" record and opposite to "opposite": ask for > 0.99 elsewhere
    let found = table
        .read::<Fact>(vec![0.0, 0.0, 1.0, 0.0], None, 0.5)
        .await
        .unwrap();

    assert!(found.is_none());
}

// ---------------------------------------------------------------------------------------------
// hybrid search: lexical channel
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn lexical_channel_finds_rare_token_that_vector_misses() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    // the vector points to record 0 (Neovim), the text asks about the video card (record 4)
    let found = table
        .search::<Fact>(axis(DIM, 0), lexical_opts("RTX 4070 драйвера"))
        .await
        .unwrap();

    let got = ids(&found);
    assert!(got.contains(&BASE_ID), "vector hit is kept");
    assert!(got.contains(&(BASE_ID + 4)), "lexical-only hit is admitted");

    let lexical_hit = found.iter().find(|r| r.id == BASE_ID + 4).unwrap();
    assert!(lexical_hit.lexical > 0.0 && lexical_hit.lexical <= 1.0);
    // it is below `min_similarity`, but its similarity is still reported
    assert!(lexical_hit.similarity.unwrap() < 0.99);

    let vector_hit = found.iter().find(|r| r.id == BASE_ID).unwrap();
    assert_eq!(vector_hit.lexical, 0.0);
}

#[tokio::test]
async fn without_text_lexical_hits_are_not_added() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    let found = table
        .search::<Fact>(
            axis(DIM, 0),
            SearchOptions {
                min_similarity: 0.99,
                text: None,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![BASE_ID]);
}

#[tokio::test]
async fn blank_text_disables_lexical_channel() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    let found = table
        .search::<Fact>(axis(DIM, 0), lexical_opts("   \n\t "))
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![BASE_ID]);
    assert_eq!(found[0].lexical, 0.0);
}

#[tokio::test]
async fn record_found_by_both_channels_ranks_first_with_full_score() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    // vector and words agree on the video card
    let found = table
        .search::<Fact>(axis(DIM, 4), lexical_opts("какая у меня видеокарта RTX"))
        .await
        .unwrap();

    assert_eq!(found[0].id, BASE_ID + 4);
    assert!(found[0].score > 0.99, "first in both channels = score 1");
    assert!(found[0].lexical > 0.0);
    assert!((found[0].similarity.unwrap() - 1.0).abs() < 1e-4);
}

#[tokio::test]
async fn single_channel_hit_scores_half_when_both_channels_work() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    let found = table
        .search::<Fact>(axis(DIM, 0), lexical_opts("RTX 4070"))
        .await
        .unwrap();

    // record 0: first by vector only, record 4: first by words only -> both get ~1/2
    assert_eq!(found.len(), 2);
    for r in &found {
        assert!((r.score - 0.5).abs() < 1e-3, "score = {}", r.score);
    }
    // equal fused rank -> tie is broken by vector similarity
    assert_eq!(found[0].id, BASE_ID);
    assert_eq!(found[1].id, BASE_ID + 4);
}

#[tokio::test]
async fn vector_only_score_is_normalized_to_one() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    // the text matches nothing -> only the vector channel works and the best hit still gets 1
    let found = table
        .search::<Fact>(axis(DIM, 0), lexical_opts("погода завтра"))
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![BASE_ID]);
    assert!((found[0].score - 1.0).abs() < 1e-4);
    assert_eq!(found[0].lexical, 0.0);
}

#[tokio::test]
async fn lexical_matches_inflections_and_folds_yo() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    // "живу" ~ "живёт" (ё folded to е, common prefix)
    let found = table
        .search::<Fact>(axis(DIM, 0), lexical_opts("где я живу"))
        .await
        .unwrap();
    assert!(ids(&found).contains(&(BASE_ID + 1)));

    // "кота" ~ "кот"
    let found = table
        .search::<Fact>(axis(DIM, 0), lexical_opts("как зовут моего кота"))
        .await
        .unwrap();
    assert!(ids(&found).contains(&(BASE_ID + 2)));

    // "использую" ~ "использует", case-insensitive Latin token
    let found = table
        .search::<Fact>(
            axis(DIM, 7),
            lexical_opts("какой редактор я использую? NEOVIM"),
        )
        .await
        .unwrap();
    assert!(ids(&found).contains(&BASE_ID));
}

#[tokio::test]
async fn lexical_does_not_match_different_numbers() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("cards").await.unwrap();

    // two records differ only by the model number
    let texts = [
        "видеокарта RTX 4070",
        "видеокарта RTX 4071",
        "кофе без сахара",
        "тёмная тема",
        "кот по кличке Том",
        "редактор Neovim",
        "проект на Rust",
        "школа и сын",
        "погода в Хельсинки",
        "плоский белый кофе",
    ];
    let batch: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| (i as u64, axis(10, i), fact(t)))
        .collect();
    table.write_batch(batch).await.unwrap();

    // numbers match exactly: "4071" finds only record 1, not the neighbour "4070"
    let found = table
        .search::<Fact>(axis(10, 9), lexical_opts("4071"))
        .await
        .unwrap();
    let got = ids(&found);
    assert!(got.contains(&1));
    assert!(!got.contains(&0));

    let found = table
        .search::<Fact>(axis(10, 9), lexical_opts("4070"))
        .await
        .unwrap();
    let got = ids(&found);
    assert!(got.contains(&0));
    assert!(!got.contains(&1));

    // a number that exists nowhere carries no signal and finds nothing
    let found = table
        .search::<Fact>(axis(10, 9), lexical_opts("4072"))
        .await
        .unwrap();
    assert_eq!(ids(&found), vec![9]);
}

#[tokio::test]
async fn lexical_ignores_stop_words_and_ubiquitous_words() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    // "пользователь" is in most documents, the rest are stop words: no signal at all
    let found = table
        .search::<Fact>(axis(DIM, 0), lexical_opts("что как пользователь для это"))
        .await
        .unwrap();

    assert_eq!(ids(&found), vec![BASE_ID]);
}

#[tokio::test]
async fn lexical_channel_works_on_nested_json() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("nested").await.unwrap();

    #[derive(Serialize, Deserialize)]
    struct Nested {
        meta: Vec<String>,
        inner: Fact,
        count: u32,
    }

    let mut batch = Vec::new();
    for i in 0..5usize {
        let word = if i == 3 { "Zephyrus" } else { "ordinary" };
        batch.push((
            i as u64,
            axis(5, i),
            Nested {
                meta: vec![format!("tag{i}x"), "common".into()],
                inner: fact(word),
                count: i as u32,
            },
        ));
    }
    table.write_batch(batch).await.unwrap();

    // all string fields are searched by default, including nested ones
    let found = table
        .search::<Nested>(axis(5, 0), lexical_opts("zephyrus"))
        .await
        .unwrap();

    assert!(ids(&found).contains(&3));
    assert_eq!(found.iter().find(|r| r.id == 3).unwrap().data.count, 3);
}

#[tokio::test]
async fn lexical_fields_restrict_searched_text() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("notes").await.unwrap();

    let notes = [
        ("groceries", "milk and bread"),
        ("travel", "tickets to Lisbon"),
        ("work", "fix the Quasar bug"),
        ("music", "new vinyl records"),
        ("health", "dentist appointment"),
    ];
    let batch: Vec<_> = notes
        .iter()
        .enumerate()
        .map(|(i, (t, b))| (i as u64, axis(5, i), note(t, b)))
        .collect();
    table.write_batch(batch).await.unwrap();

    let query = |fields: Option<Vec<String>>| SearchOptions {
        min_similarity: 0.99,
        text: Some("quasar".into()),
        lexical_fields: fields,
        ..Default::default()
    };

    // the word is in `body` only
    let in_body = table
        .search::<Note>(axis(5, 0), query(Some(vec!["body".into()])))
        .await
        .unwrap();
    assert!(ids(&in_body).contains(&2));

    let in_title = table
        .search::<Note>(axis(5, 0), query(Some(vec!["title".into()])))
        .await
        .unwrap();
    assert!(!ids(&in_title).contains(&2));

    // unknown field -> nothing to search
    let unknown = table
        .search::<Note>(axis(5, 0), query(Some(vec!["nope".into()])))
        .await
        .unwrap();
    assert!(!ids(&unknown).contains(&2));

    // None = all string fields
    let all = table.search::<Note>(axis(5, 0), query(None)).await.unwrap();
    assert!(ids(&all).contains(&2));
}

#[tokio::test]
async fn lexical_scores_are_normalized() {
    let (_dir, ctx) = setup().await;
    let table = corpus_table(&ctx).await;

    let found = table
        .search::<Fact>(
            axis(DIM, 4),
            lexical_opts("какая у меня видеокарта RTX 4070"),
        )
        .await
        .unwrap();

    for r in &found {
        assert!(r.lexical >= 0.0 && r.lexical <= 1.0);
        assert!(r.score >= 0.0 && r.score <= 1.0);
    }
}

#[tokio::test]
async fn lexical_hits_respect_limit() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("rust").await.unwrap();

    // many records share a rare word; a limit must still cut the fused result
    let mut batch = Vec::new();
    for i in 0..30u64 {
        let text = if i < 8 {
            "заметка про Rust"
        } else {
            "что-то другое"
        };
        batch.push((i, axis(30, i as usize), fact(text)));
    }
    table.write_batch(batch).await.unwrap();

    // 8 of 30 documents = 27% (< 40%), so "rust" is a specific word
    let found = table
        .search::<Fact>(
            axis(30, 20),
            SearchOptions {
                limit: 3,
                min_similarity: 0.99,
                text: Some("rust".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(found.len(), 3);
}

// ---------------------------------------------------------------------------------------------
// index
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn ensure_index_is_noop_on_small_tables_and_search_still_works() {
    let (_dir, ctx) = setup().await;
    let table = vector_table(&ctx).await;

    table.ensure_index().await.unwrap();
    table.ensure_index().await.unwrap();

    let found = table
        .search::<Fact>(vec![1.0, 0.0, 0.0, 0.0], SearchOptions::default())
        .await
        .unwrap();
    assert_eq!(found[0].id, 1);
}

// ---------------------------------------------------------------------------------------------
// Context (database level)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn data_persists_between_connections() {
    let dir = TempDir::new();

    {
        let ctx = Context::connect(dir.path()).await.unwrap();
        let table = ctx.open_table("facts").await.unwrap();
        table.write(1, axis(4, 0), fact("persisted")).await.unwrap();
    }

    let ctx = Context::connect(dir.path()).await.unwrap();
    let table = ctx.open_table("facts").await.unwrap();
    let all = table.read_all::<Fact>().await.unwrap().unwrap();

    assert_eq!(all.len(), 1);
    assert_eq!(all[0].data, fact("persisted"));
}

#[tokio::test]
async fn tables_are_isolated() {
    let (_dir, ctx) = setup().await;
    let a = ctx.open_table("a").await.unwrap();
    let b = ctx.open_table("b").await.unwrap();

    a.write(1, axis(4, 0), fact("in a")).await.unwrap();
    b.write(2, axis(4, 1), fact("in b")).await.unwrap();

    let in_a = a.read_all::<Fact>().await.unwrap().unwrap();
    let in_b = b.read_all::<Fact>().await.unwrap().unwrap();
    assert_eq!((in_a.len(), in_a[0].id), (1, 1));
    assert_eq!((in_b.len(), in_b[0].id), (1, 2));

    // different tables may even have different embedding dimensions
    b.write(3, axis(4, 2), fact("more b")).await.unwrap();
    assert_eq!(b.read_all::<Fact>().await.unwrap().unwrap().len(), 2);
}

#[tokio::test]
async fn cloned_handles_share_the_same_data() {
    let (_dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    let clone = table.clone();

    table.write(1, axis(4, 0), fact("one")).await.unwrap();

    assert_eq!(clone.read_all::<Fact>().await.unwrap().unwrap().len(), 1);

    let ctx2 = ctx.clone();
    let same = ctx2.open_table("facts").await.unwrap();
    assert_eq!(same.read_all::<Fact>().await.unwrap().unwrap().len(), 1);
}

#[tokio::test]
async fn remove_table_deletes_its_data() {
    let (dir, ctx) = setup().await;
    let table = ctx.open_table("facts").await.unwrap();
    table.write(1, axis(4, 0), fact("one")).await.unwrap();
    assert!(dir.path().join("facts.lance").exists());

    ctx.remove_table("facts").await.unwrap();

    assert!(!dir.path().join("facts.lance").exists());

    // a fresh connection sees no table
    let ctx = Context::connect(dir.path()).await.unwrap();
    let table = ctx.open_table("facts").await.unwrap();
    assert!(table.read_all::<Fact>().await.unwrap().is_none());
}

#[tokio::test]
async fn remove_unknown_table_is_ok() {
    let (_dir, ctx) = setup().await;

    ctx.remove_table("does-not-exist").await.unwrap();
}

#[tokio::test]
async fn remove_table_keeps_other_tables() {
    let (dir, ctx) = setup().await;
    let a = ctx.open_table("a").await.unwrap();
    let b = ctx.open_table("b").await.unwrap();
    a.write(1, axis(4, 0), fact("a")).await.unwrap();
    b.write(2, axis(4, 1), fact("b")).await.unwrap();

    ctx.remove_table("a").await.unwrap();

    let ctx = Context::connect(dir.path()).await.unwrap();
    let b = ctx.open_table("b").await.unwrap();
    assert_eq!(b.read_all::<Fact>().await.unwrap().unwrap().len(), 1);
}
