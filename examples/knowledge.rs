use cistern::{Context, Record};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DocumentChunk {
    text: String,
    source: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    // 1. Connect to database:
    let ctx = Context::connect("./database").await?;
    let table = "documents";

    // 2. Prepare data:
    let chunk_1 = DocumentChunk {
        text: "Rust is a programming language that ensures memory safety.".to_string(),
        source: "book.pdf".to_string(),
    };
    let chunk_2 = DocumentChunk {
        text: "LanceDB uses the Lance format for fast vector search.".to_string(),
        source: "wiki.md".to_string(),
    };

    let batch = vec![
        (vec![0.1, 0.2, 0.3, 0.4], chunk_1),
        (vec![0.5, 0.6, 0.7, 0.8], chunk_2),
    ];

    // 3. Write data:
    ctx.write_batch(table, batch).await?;

    // 4. Search data:
    let query_vector = vec![0.1, 0.2, 0.25, 0.35];
    if let Some(results) = ctx
        .read::<DocumentChunk>(table, query_vector, 2, 0.85)
        .await?
    {
        for Record { id, data } in results {
            let DocumentChunk { source, text } = data;
            println!("[{id}] {source} — {text}");
        }
    }

    // 5. Optimize search index (IVF-PQ):
    ctx.optimize_index(table, 256, 16).await?;

    Ok(())
}
