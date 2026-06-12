use crate::prelude::*;
use std::path::Path;

pub trait Backend: Clone + Send + Sync + 'static {
    type T;

    /// The logic of connecting to a disk
    fn connect(dir: &Path) -> impl std::future::Future<Output = Result<Self>> + Send;

    /// The logic of opening a specific data bay
    fn open_table(&self, name: &str) -> impl std::future::Future<Output = Result<Self::T>> + Send;

    /// The logic of removing a specific data bay
    fn remove_table(&self, name: &str) -> impl std::future::Future<Output = Result<()>> + Send;
}
