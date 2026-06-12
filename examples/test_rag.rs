#![cfg(feature = "rag")]
use cistern::{Cistern, Rag, RagRecord};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Document {
    text: String,
    source: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    // connect to db:
    let db = Cistern::<Rag>::connect(".database").await?;
    let docs = db.open_table("documents").await?;

    // write data:
    docs.write(
        vec![0.1, 0.2, 0.3, 0.4],
        Document {
            text: "Rust is a programming language that ensures memory safety.".to_string(),
            source: "book.pdf".to_string(),
        },
    )
    .await?;

    // write batch data:
    docs.write_batch(vec![
        (
            vec![0.2, 0.7, 0.3, 0.5],
            Document {
                text: "Async Rust empowers developers to write highly performant, scalable, and responsive applications.".to_string(),
                source: "docs.md".to_string(),
            },
        ),
        (
            vec![0.5, 0.6, 0.7, 0.8],
            Document {
                text: "LanceDB uses the Lance format for fast vector search.".to_string(),
                source: "wiki.md".to_string(),
            },
        ),
    ])
    .await?;

    // read data:
    if let Some(records) = docs.read(vec![0.1, 0.2, 0.25, 0.35], 10, 0.85).await? {
        for RagRecord { id, data } in records {
            let Document { source, text } = data;
            println!("[{id}] {source} — {text}");
        }
    }

    // optimize table indexing:
    docs.index(256, 16).await?;

    // remove table:
    db.remove_table("documents").await?;

    Ok(())
}
