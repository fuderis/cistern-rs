use macron::{Display, Error, From};

/// The error
#[derive(Debug, Display, Error, From)]
pub enum Error {
    #[display = "Expected '{0}' column (maybe table is broken)"]
    ExpectedColumn(&'static str),

    #[display = "Failed to downcast '{0}' to {1}"]
    FailedDowncast(&'static str, &'static str),

    #[display = "All vectors in a batch must have the same length"]
    InvalidBatchLength,
}
