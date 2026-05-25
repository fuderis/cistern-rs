[![github]](https://github.com/fuderis/cistern-rs)&ensp;
[![crates-io]](https://crates.io/crates/cistern)&ensp;
[![docs-rs]](https://docs.rs/cistern)

[github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
[crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
[docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs

# Cistern: A Friendly RAG Framework based on LanceDB

Cistern is a high-level, asynchronous RAG (Retrieval-Augmented Generation) framework built on top of LanceDB and Apache Arrow. 
It abstracts away the complex, low-level mechanics of Arrow arrays and vector indexing into a clean, developer-friendly API
designed for building robust AI-driven applications.

Whether you are building a production-grade semantic search engine or a local LLM knowledge base, Cistern acts as a reliable,
monolithic reservoir—allowing you to fluidly "pour in" raw data chunks and "draw out" precise context seamlessly.

## Features:

* **Clean Type Abstraction**
  Say goodbye to manual Arrow `RecordBatch` construction and type downcasting. Cistern completely wraps `arrow_array` complexities, allowing you to work with native Rust structs and types using standard `serde` serialization.
* **High-Performance Bulk Insertion**
  Features an idiomatic `write_batch` implementation that takes data as cohesive pairs (`Vec<(Vec<f32>, T)>`). It instantly flattens high-dimensional vectors into Apache Arrow's memory layout in a single pass, speeding up data ingestion by up to 10x compared to atomic writes.
* **Race-Condition Safety**
  Engineered for highly concurrent environments. Table creation uses strict `CreateTableMode::Create` verification under the hood, ensuring multiple async workers never silently overwrite each other's data during a cold start.
* **Collision-Free Distributed IDs**
  Generates high-performance 64-bit (`u64`) keys utilizing a Snowflake-like architecture (42 bits for time, 22 bits for crypto-safe randomness). Even during lightning-fast bulk imports, IDs are sequentially padded to mathematically guarantee uniqueness without the overhead of string UUIDs.
* **Smart Search Optimization**
  Built-in support for **IVF-PQ** (Inverted File with Product Quantization) index tuning. Cistern accelerates vector lookups on large-scale datasets by partitioning embedding spaces into quantization clusters, while gracefully and silently bypassing index builds on small datasets to prevent cold-start crashes.
* **Thread-Safe & Clonable**
  The main `Context` structure natively encapsulates LanceDB's thread-safe connections. It is fully `Send + Sync` and cheaply clonable out of the box, making it trivial to inject into state managers for web frameworks like `Axum` or `Actix-web`.

## Examples:

### Quick start:
```rust
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
```

## License & Feedback:

> This library distributed under the [MIT](https://github.com/fuderis/cistern-rs/blob/main/LICENSE.md) license.

You can contact me via [GitHub](https://github.com/fuderis) or send a message to my [E-Mail](mailto:synapdrake@ya.ru).
This library is actively evolving, and your suggestions and feedback are always welcome!
