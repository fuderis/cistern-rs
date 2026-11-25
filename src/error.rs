use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum Error {
    Io(std::io::Error),

    Json(Box<serde_json::Error>),

    Join(tokio::task::JoinError),

    #[cfg(feature = "context")]
    Context(crate::ContextError),

    #[cfg(feature = "storage")]
    Storage(crate::StorageError),
}
