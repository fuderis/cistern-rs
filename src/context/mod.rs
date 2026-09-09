//! RAG database module.

pub mod record;
pub use record::Record;

pub mod table;
pub use table::Table;

pub use arrow_array;
pub use arrow_schema;
pub use lancedb;
pub use serde_arrow;

use crate::prelude::*;

/// RAG context database (based on LanceDB).
#[derive(Clone)]
pub struct Context {
    conn: Arc<lancedb::Connection>,
    dir: PathBuf,
}

impl Context {
    /// Connects to database.
    pub async fn connect(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        let uri = dir.to_string_lossy().to_string();
        let conn = lancedb::connect(&uri).execute().await?;

        Ok(Self {
            conn: Arc::new(conn),
            dir,
        })
    }

    /// Opens database table.
    pub async fn open_table(&self, name: &str) -> Result<Table> {
        Ok(Table::new(self.conn.clone(), name))
    }

    /// Removes database table.
    pub async fn remove_table(&self, name: &str) -> Result<()> {
        let table_path = self.dir.join(str!("{}.lance", name));
        if table_path.exists() {
            tokio::fs::remove_dir_all(&table_path).await?;
        }

        Ok(())
    }
}
