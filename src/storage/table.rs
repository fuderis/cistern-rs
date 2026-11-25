use super::StorageError;
use crate::prelude::*;

use serde::de::DeserializeOwned;
use tokio::task;

/// Key-value database table (Sled).
#[derive(Clone)]
pub struct StorageTable {
    tree: sled::Tree,
}

impl StorageTable {
    /// Creates new table instance.
    pub(crate) fn new(tree: sled::Tree) -> Self {
        Self { tree }
    }

    /// Reads value by any serializable key.
    pub async fn read<K, V>(&self, key: K) -> Result<Option<V>, Error>
    where
        K: Serialize + Send + 'static,
        V: DeserializeOwned + Send + 'static,
    {
        let tree = self.tree.clone();

        task::spawn_blocking(move || -> Result<Option<V>, Error> {
            let key_bytes = serde_json::to_vec(&key)?;

            if let Some(bytes) = tree.get(key_bytes).map_err(StorageError::from)? {
                let value = serde_json::from_slice(&bytes)?;
                return Ok(Some(value));
            }
            Ok(None)
        })
        .await?
    }

    /// Reads all records from the table.
    pub async fn read_all<K, V>(&self) -> Result<Vec<(K, V)>, Error>
    where
        K: DeserializeOwned + Send + 'static,
        V: DeserializeOwned + Send + 'static,
    {
        let tree = self.tree.clone();

        task::spawn_blocking(move || -> Result<Vec<(K, V)>, Error> {
            let mut items = Vec::new();

            for res in tree.iter() {
                let (key_bytes, value_bytes) = res.map_err(StorageError::from)?;
                let key: K = serde_json::from_slice(&key_bytes)?;
                let value: V = serde_json::from_slice(&value_bytes)?;
                items.push((key, value));
            }

            Ok(items)
        })
        .await?
    }

    /// Writes any serializable data to the table.
    pub async fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: Serialize + Send + 'static,
        V: Serialize + Send + 'static,
    {
        let tree = self.tree.clone();

        task::spawn_blocking(move || -> Result<(), Error> {
            let key_bytes = serde_json::to_vec(&key)?;
            let value_bytes = serde_json::to_vec(&value)?;

            tree.insert(key_bytes, value_bytes)
                .map_err(StorageError::from)?;
            Ok(())
        })
        .await?
    }

    /// Flushes all cached data from RAM to the disk.
    pub async fn flush(&self) -> Result<(), Error> {
        let tree = self.tree.clone();

        task::spawn_blocking(move || -> Result<(), Error> {
            tree.flush().map_err(StorageError::from)?;
            Ok(())
        })
        .await?
    }

    /// Removes table record by any serializable key.
    pub async fn remove<K>(&self, key: K) -> Result<(), Error>
    where
        K: Serialize + Send + 'static,
    {
        let tree = self.tree.clone();

        task::spawn_blocking(move || -> Result<(), Error> {
            let key_bytes = serde_json::to_vec(&key)?;

            tree.remove(key_bytes).map_err(StorageError::from)?;
            Ok(())
        })
        .await?
    }

    /// Clears all records in the table.
    pub async fn clear(&self) -> Result<(), Error> {
        let tree = self.tree.clone();

        task::spawn_blocking(move || -> Result<(), Error> {
            tree.clear().map_err(StorageError::from)?;
            Ok(())
        })
        .await?
    }
}
