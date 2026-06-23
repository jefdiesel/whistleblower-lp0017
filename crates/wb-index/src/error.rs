//! Error types for the indexing pipeline. Each layer has its own error so
//! callers can react precisely (e.g. retry only transient storage failures).

use thiserror::Error;

/// Errors from the Logos Storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage transport error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("storage returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("storage returned an empty CID")]
    EmptyCid,
}

impl StorageError {
    /// Whether this failure is worth retrying with backoff. Network/5xx are
    /// transient; a 4xx (bad request) or empty-CID is not.
    pub fn is_transient(&self) -> bool {
        match self {
            StorageError::Http(e) => {
                e.is_timeout() || e.is_connect() || e.is_request() || e.is_body()
            }
            StorageError::Status { status, .. } => *status >= 500 || *status == 429,
            StorageError::EmptyCid => false,
        }
    }
}

/// Errors from the Logos Delivery layer.
#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error("delivery transport error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("delivery returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("invalid delivery payload: {0}")]
    Payload(String),
}

/// Errors from the on-chain registry layer.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("registry transport error: {0}")]
    Transport(String),
    #[error("registry rejected the transaction: {0}")]
    Rejected(String),
    #[error("failed to decode registry data: {0}")]
    Decode(String),
}

/// Top-level error for high-level pipeline operations.
#[derive(Debug, Error)]
pub enum IndexError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Delivery(#[from] DeliveryError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("invalid envelope: {0}")]
    Envelope(#[from] wb_types::EnvelopeError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("upload failed after {attempts} attempt(s): {source}")]
    UploadRetriesExhausted {
        attempts: u32,
        #[source]
        source: StorageError,
    },
}
