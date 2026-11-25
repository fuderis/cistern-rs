use crate::prelude::*;

/// Embeddings record data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRecord<T> {
    pub id: u64,
    pub data: T,
}
