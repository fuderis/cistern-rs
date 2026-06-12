use crate::prelude::*;

/// The embeddings record data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagRecord<T> {
    pub id: u64,
    pub data: T,
}
