use crate::prelude::*;

/// Embeddings record data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record<T> {
    pub id: u64,
    pub data: T,
}
