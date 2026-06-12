use crate::prelude::*;
use serde::de::DeserializeOwned;

/// The key-value database table (Sled)
#[derive(Clone)]
pub struct KvTable {
    tree: sled::Tree,
}

impl KvTable {
    /// Creates a new table instance
    pub(crate) fn new(tree: sled::Tree) -> Self {
        Self { tree }
    }

    /// Reads a value by any serializable key
    pub async fn read<K, V>(&self, key: K) -> Result<Option<V>>
    where
        K: Serialize + Send + 'static,
        V: DeserializeOwned + Send + 'static,
    {
        let tree = self.tree.clone();

        tokio::task::spawn_blocking(move || -> Result<Option<V>> {
            let key_bytes = serde_json::to_vec(&key)?;

            if let Some(bytes) = tree.get(key_bytes)? {
                let value = serde_json::from_slice(&bytes)?;
                return Ok(Some(value));
            }
            Ok(None)
        })
        .await?
    }

    /// Writes any serializable data to the table by any serializable key
    pub async fn write<K, V>(&self, key: K, value: V) -> Result<()>
    where
        K: Serialize + Send + 'static,
        V: Serialize + Send + 'static,
    {
        let tree = self.tree.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let key_bytes = serde_json::to_vec(&key)?;
            let value_bytes = serde_json::to_vec(&value)?;

            tree.insert(key_bytes, value_bytes)?;
            Ok(())
        })
        .await?
    }

    /// Forcibly flushes all cached data from RAM to the physical disk
    pub async fn flush(&self) -> Result<()> {
        let tree = self.tree.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            tree.flush()?;
            Ok(())
        })
        .await?
    }

    /// Removes a table record by any serializable key
    pub async fn remove<K>(&self, key: K) -> Result<()>
    where
        K: Serialize + Send + 'static,
    {
        let tree = self.tree.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let key_bytes = serde_json::to_vec(&key)?;

            tree.remove(key_bytes)?;
            Ok(())
        })
        .await?
    }
}
