#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
pub mod error;
pub mod prelude;

pub mod context;
pub use context::{Context, Record};

pub use arrow_array;
pub use arrow_schema;
pub use lancedb;
pub use serde_arrow;
