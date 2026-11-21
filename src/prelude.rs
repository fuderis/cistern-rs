#![allow(unused_imports)]

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

pub use crate::error::Error;

pub use atoman::StreamExt;

pub use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub use serde::{Deserialize, Serialize};
pub use serde_json::{self as json, Value as JsonValue, json};
