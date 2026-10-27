//! Domain entities (pure business objects)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tenant in the multi-tenant SaaS system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub contract_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_active: bool,
    /// Optional API key hash (sha256 hex) for auth
    #[serde(default)]
    pub api_key_hash: Option<String>,
    /// Billing plan limits
    #[serde(default)]
    pub plan: TenantPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantPlan {
    /// Max contract calls per calendar day
    pub daily_call_limit: u64,
    /// Max cumulative gas per calendar day
    pub daily_gas_limit: u64,
    /// Max concurrent in-flight blockchain calls
    pub max_concurrency: u32,
}

impl Default for TenantPlan {
    fn default() -> Self {
        Self {
            daily_call_limit: 10_000,
            daily_gas_limit: 50_000_000,
            max_concurrency: 16,
        }
    }
}

/// Smart contract call request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCallRequest {
    pub tenant_id: Uuid,
    pub contract_address: String,
    pub method: String,
    pub args: serde_json::Value,
    pub caller: Option<String>,
}

/// Result of a contract call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCallResult {
    pub request_id: Uuid,
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
    pub gas_used: Option<u64>,
    pub block_hash: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Deployment request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    pub tenant_id: Uuid,
    pub wasm_path: String,
    pub constructor: String,
    pub args: serde_json::Value,
}

/// Health / node status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub connected: bool,
    pub chain: String,
    pub block_number: u64,
    pub peers: u32,
    pub last_checked: DateTime<Utc>,
}

/// Usage record for billing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub tenant_id: Uuid,
    pub calls: u64,
    pub gas_used: u64,
    pub day: String, // YYYY-MM-DD
}

/// Auth principal extracted from API key / JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub tenant_id: Uuid,
    pub subject: String,
    pub roles: Vec<String>,
}
