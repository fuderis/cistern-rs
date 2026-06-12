#![cfg(feature = "kv")]
use cistern::{Cistern, Kv};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Document {
    text: String,
    source: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    // connect to db:
    let db = Cistern::<Kv>::connect(".database").await?;
    let docs = db.open_table("documents").await?;

    // write data:
    docs.write(
        "rust-book",
        Document {
            text: "Rust is a programming language that ensures memory safety.".to_string(),
            source: "book.pdf".to_string(),
        },
    )
    .await?;

    docs.write(
        "async-rust",
        Document {
                text: "Async Rust empowers developers to write highly performant, scalable, and responsive applications.".to_string(),
                source: "docs.md".to_string(),
            },
    )
    .await?;

    // read data:
    if let Some(value) = docs.read("rust-book").await? {
        let Document { source, text } = value;
        println!("[rust-book] {source} — {text}");
    }

    // remove data:
    docs.remove("async-rust").await?;
    let result = docs.read::<_, Document>("async-rust").await?;
    assert!(result.is_none());

    // force flush cache from memory to disk:
    docs.flush().await?;

    // remove table:
    db.remove_table("documents").await?;

    Ok(())
}
