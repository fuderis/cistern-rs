use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generates unique u64 (based on current time and atomic counter).
pub fn gen_id() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;

    let count = COUNTER.fetch_add(1, Ordering::Relaxed) % 1000;

    // shift ms to free up 10 bits at the bottom for a local counter (up to 1000 values/ms)
    (millis << 10) | count
}
