//! Key-Value database module.

pub mod table;
pub use table::Table;

pub use sled;

use crate::prelude::*;

/// Key-Value storage database based on Sled
#[derive(Clone)]
pub struct Storage {
    db: sled::Db,
}

impl Storage {
    /// Connects to database.
    pub async fn connect(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        let db = sled::open(dir)?;

        Ok(Self { db })
    }

    /// Opens database table.
    pub async fn open_table(&self, name: &str) -> Result<Table> {
        let tree = self.db.open_tree(name)?;
        Ok(Table::new(tree))
    }

    /// Removes database table.
    pub async fn remove_table(&self, name: &str) -> Result<()> {
        self.db.drop_tree(name)?;
        Ok(())
    }
}
