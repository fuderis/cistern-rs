[![github]](https://github.com/fuderis/cistern-rs)&ensp;
[![crates-io]](https://crates.io/crates/cistern)&ensp;
[![docs-rs]](https://docs.rs/cistern)

[github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
[crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
[docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs

# Cistern: A Friendly Framework based on LanceDB & Sled

Cistern is a high-level, asynchronous storage abstraction layer built for AI-driven applications, LLM orchestrators, and local agents.
It acts as a reliable, zero-boilerplate reservoir that unifies heavy semantic vector search and lightning-fast key-value state
management under a single architectural pattern.<br>

Instead of wrestling with low-level database configurations, raw byte arrays, or memory-layout mapping,
Cistern leverages standard Rust types and `serde` serialization to expose a clean, developer-friendly API.

## Features:

* **Modular Backend Architecture:** Choose between heavy semantic processing (`Rag`) or high-performance
  state management (`Kv`) via feature flags, completely isolating compile-time dependencies.
* **Clean Type Abstraction:** No manual Arrow `RecordBatch` construction or raw key-to-byte serialization.
  Work natively with your own Rust structures.
* **Thread-Safe & Cheaply Clonable:** The core `Cistern<B>` engine handles internal connection pooling.
  It is fully `Send + Sync` and can be easily shared across `tokio` threads or web frameworks like `Axum`.
* **Zero Cross-Contamination:** Built around clean Rust trait boundaries. If you only use the KV engine,
  heavy dependencies like LanceDB and Apache Arrow won't even compile into your binary.

## Installation:

* To use only Key-Value (Sled)
```bash
cargo add cistern --features kv
```

* To use Vector Search only (LanceDB)
```bash
cargo add cistern --features rag
```

* Or all of them together
```bash
cargo add cistern --features full
```

## Examples:

### Rag (LanceDB) [feature: `rag`]:

**Powered by LanceDB** and **Apache Arrow**. Perfect for semantic knowledge bases, long-term agent memory, and chunk retrieval.
Features automated distributed 64-bit ID padding and integrated IVF-PQ index tuning.

```rust
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
```

### Key-Value (SledDB) [feature: `kv`]:

**Powered by Sled**. Built for uncompromising speed and ultra-low latency. Engineered specifically for real-time AI
session tracking, sub-millisecond context swapping and internal tool caching.

```rust
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
```

## License & Feedback:

> This library distributed under the [MIT](https://github.com/fuderis/cistern-rs/blob/main/LICENSE.md) license.

You can contact me via [GitHub](https://github.com/fuderis) or send a message to my [E-Mail](mailto:synapdrake@ya.ru).
This library is actively evolving, and your suggestions and feedback are always welcome!
