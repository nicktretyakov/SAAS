//! Ports (interfaces) — Dependency Inversion & Interface Segregation

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::*;
use crate::error::AdapterError;

/// Port for interacting with the blockchain node
#[async_trait]
pub trait BlockchainPort: Send + Sync {
    async fn get_status(&self) -> Result<NodeStatus, AdapterError>;
    async fn call_contract(&self, request: ContractCallRequest) -> Result<ContractCallResult, AdapterError>;
    async fn deploy_contract(&self, request: DeployRequest) -> Result<String, AdapterError>;
    async fn get_storage(&self, contract: &str, key: &str) -> Result<Option<serde_json::Value>, AdapterError>;
}

/// Port for tenant persistence
#[async_trait]
pub trait TenantRepository: Send + Sync {
    async fn create(&self, tenant: Tenant) -> Result<Tenant, AdapterError>;
    async fn get(&self, id: Uuid) -> Result<Option<Tenant>, AdapterError>;
    async fn update(&self, tenant: Tenant) -> Result<Tenant, AdapterError>;
    async fn list_active(&self) -> Result<Vec<Tenant>, AdapterError>;
    /// Lookup by API key hash (sha256 hex)
    async fn get_by_api_key_hash(&self, key_hash: &str) -> Result<Option<Tenant>, AdapterError>;
}

/// Port for caching
#[async_trait]
pub trait CachePort: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>, AdapterError>;
    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), AdapterError>;
    async fn invalidate(&self, key: &str) -> Result<(), AdapterError>;
}

/// Port for event publishing
#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event_type: &str, payload: serde_json::Value) -> Result<(), AdapterError>;
}

/// Port for billing / usage metering
#[async_trait]
pub trait BillingPort: Send + Sync {
    /// Reserve capacity before a call; Err if over limit
    async fn check_and_reserve(&self, tenant_id: Uuid, plan: &TenantPlan) -> Result<(), AdapterError>;
    /// Record actual usage after a call
    async fn record_usage(&self, tenant_id: Uuid, gas_used: u64) -> Result<(), AdapterError>;
    /// Current day usage
    async fn get_usage(&self, tenant_id: Uuid) -> Result<UsageRecord, AdapterError>;
}
