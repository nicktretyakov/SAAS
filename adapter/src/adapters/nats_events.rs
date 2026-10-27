//! EventPublisher → NATS JetStream / core NATS
//! Enable with feature `nats`.

#![cfg(feature = "nats")]

use async_trait::async_trait;

use crate::error::AdapterError;
use crate::ports::EventPublisher;

pub struct NatsEventPublisher {
    client: async_nats::Client,
    /// Subject prefix, e.g. "saas.events"
    prefix: String,
}

impl NatsEventPublisher {
    pub async fn connect(nats_url: &str, prefix: impl Into<String>) -> Result<Self, AdapterError> {
        let client = async_nats::connect(nats_url)
            .await
            .map_err(|e| AdapterError::Internal(format!("nats connect: {}", e)))?;
        Ok(Self {
            client,
            prefix: prefix.into(),
        })
    }
}

#[async_trait]
impl EventPublisher for NatsEventPublisher {
    async fn publish(&self, event_type: &str, payload: serde_json::Value) -> Result<(), AdapterError> {
        let subject = format!("{}.{}", self.prefix, event_type.replace('.', "_"));
        let bytes = serde_json::to_vec(&payload)
            .map_err(|e| AdapterError::Internal(e.to_string()))?;
        self.client
            .publish(subject, bytes.into())
            .await
            .map_err(|e| AdapterError::Internal(format!("nats publish: {}", e)))?;
        Ok(())
    }
}
