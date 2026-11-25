#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
mod prelude;

pub mod error;
pub use error::Error;

#[cfg(feature = "context")]
pub mod context;
#[cfg(feature = "context")]
pub use context::{Context, ContextError, ContextRecord, ContextTable};

#[cfg(feature = "storage")]
pub mod storage;
#[cfg(feature = "storage")]
pub use storage::{Storage, StorageError, StorageTable};

pub mod gen_id;
pub use gen_id::gen_id;
