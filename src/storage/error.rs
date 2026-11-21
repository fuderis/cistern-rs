use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum Error {
    Sled(sled::Error),
}
