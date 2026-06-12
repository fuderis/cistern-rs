pub mod record;
pub use record::RagRecord;

pub mod table;
pub use table::RagTable;

pub use arrow_array;
pub use arrow_schema;
pub use lancedb;
pub use serde_arrow;

use crate::{Backend, prelude::*};

/// The RAG database based on LanceDB
#[derive(Clone)]
pub struct Rag {
    conn: Arc<lancedb::Connection>,
    dir: PathBuf,
}

impl Backend for Rag {
    type T = RagTable;

    async fn connect(dir: &Path) -> Result<Self> {
        let dir = dir.to_path_buf();
        let uri = dir.to_string_lossy().to_string();
        let conn = lancedb::connect(&uri).execute().await?;

        Ok(Self {
            conn: Arc::new(conn),
            dir,
        })
    }

    async fn open_table(&self, name: &str) -> Result<Self::T> {
        Ok(RagTable::new(self.conn.clone(), name))
    }

    async fn remove_table(&self, name: &str) -> Result<()> {
        let table_path = self.dir.join(str!("{}.lance", name));
        if table_path.exists() {
            tokio::fs::remove_dir_all(&table_path).await?;
        }

        Ok(())
    }
}
