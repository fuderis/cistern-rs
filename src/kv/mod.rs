pub mod table;
pub use table::KvTable;

pub use sled;

use crate::{Backend, prelude::*};

/// The Key-Value database based on Sled
#[derive(Clone)]
pub struct Kv {
    db: sled::Db,
}

impl Backend for Kv {
    type T = KvTable;

    async fn connect(dir: &Path) -> Result<Self> {
        let dir = dir.to_path_buf();
        let db = sled::open(dir)?;

        Ok(Self { db })
    }

    async fn open_table(&self, name: &str) -> Result<Self::T> {
        let tree = self.db.open_tree(name)?;
        Ok(KvTable::new(tree))
    }

    async fn remove_table(&self, name: &str) -> Result<()> {
        self.db.drop_tree(name)?;
        Ok(())
    }
}
