pub mod backend;
pub use backend::Backend;

use crate::prelude::*;

/// The database wrapper
#[derive(Clone)]
pub struct Cistern<B: Backend> {
    backend: B,
}

impl<B: Backend> Cistern<B> {
    /// Connects to the database dir
    pub async fn connect(dir: impl AsRef<Path>) -> Result<Self> {
        let backend = B::connect(dir.as_ref()).await?;
        Ok(Self { backend })
    }

    /// Opens the database table
    pub async fn open_table(&self, name: &str) -> Result<B::T> {
        self.backend.open_table(name).await
    }

    /// Removes the database table
    pub async fn remove_table(&self, name: &str) -> Result<()> {
        self.backend.remove_table(name).await
    }
}
