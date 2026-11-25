use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum StorageError {
    Sled(sled::Error),
}
