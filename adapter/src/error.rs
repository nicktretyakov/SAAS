//! Unified error type

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AdapterError {
    #[error("Blockchain connection error: {0}")]
    Connection(String),

    #[error("Contract call failed: {0}")]
    ContractCall(String),

    #[error("Deployment failed: {0}")]
    Deployment(String),

    #[error("Tenant not found: {0}")]
    TenantNotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Timeout")]
    Timeout,
}

impl From<serde_json::Error> for AdapterError {
    fn from(e: serde_json::Error) -> Self {
        AdapterError::Internal(e.to_string())
    }
}
