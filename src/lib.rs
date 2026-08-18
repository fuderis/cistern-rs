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

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generates a unique u64 based on the current time and an atomic counter.
/// Guarantees uniqueness and monotonicity even with frequent calls in the same thread/asynchronous environment.
pub fn generate_id() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;

    let count = COUNTER.fetch_add(1, Ordering::Relaxed) % 1000;

    // shift the milliseconds to free up 10 bits at the bottom for a local counter (up to 1000 values/ms)
    (millis << 10) | count
}
