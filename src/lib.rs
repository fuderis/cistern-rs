#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
pub mod error;
pub mod prelude;

pub mod cistern;
pub use cistern::{Backend, Cistern};

#[cfg(feature = "rag")]
pub mod rag;
#[cfg(feature = "rag")]
pub use rag::{Rag, RagRecord, RagTable};

#[cfg(feature = "kv")]
pub mod kv;
#[cfg(feature = "kv")]
pub use kv::{Kv, KvTable};
