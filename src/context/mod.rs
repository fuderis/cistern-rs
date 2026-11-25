//! RAG database module.

pub mod error;
pub use error::ContextError;

pub mod record;
pub use record::ContextRecord;

pub mod table;
pub use table::ContextTable;

pub use arrow_array;
pub use arrow_schema;
pub use lancedb;
pub use serde_arrow;

use crate::prelude::*;
use tokio::fs;

/// RAG context database (based on LanceDB).
#[derive(Clone)]
pub struct Context {
    conn: Arc<lancedb::Connection>,
    dir: PathBuf,
}

impl Context {
    /// Connects to database.
    pub async fn connect(dir: impl Into<PathBuf>) -> Result<Self, Error> {
        let dir = dir.into();
        let uri = dir.to_string_lossy().to_string();
        let conn = lancedb::connect(&uri)
            .execute()
            .await
            .map_err(ContextError::from)?;

        Ok(Self {
            conn: Arc::new(conn),
            dir,
        })
    }

    /// Opens database table.
    pub async fn open_table(&self, name: &str) -> Result<ContextTable, Error> {
        Ok(ContextTable::new(self.conn.clone(), name))
    }

    /// Removes database table.
    pub async fn remove_table(&self, name: &str) -> Result<(), Error> {
        let table_path = self.dir.join(format!("{}.lance", name));
        if table_path.exists() {
            fs::remove_dir_all(&table_path).await?;
        }

        Ok(())
    }
}
