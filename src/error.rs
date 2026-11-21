use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum Error {
    Io(std::io::Error),

    Json(Box<serde_json::Error>),

    Join(atoman::task::JoinError),

    #[cfg(feature = "context")]
    Context(crate::context::error::Error),

    #[cfg(feature = "storage")]
    Storage(crate::storage::error::Error),
}
