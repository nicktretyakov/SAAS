//! Concrete adapters implementing the ports

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::domain::*;
use crate::error::AdapterError;
use crate::ports::*;

// ============================================================================
// In-memory Tenant Repository (thread-safe via DashMap)
// ============================================================================

pub struct InMemoryTenantRepository {
    store: DashMap<Uuid, Tenant>,
}

impl InMemoryTenantRepository {
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
}

impl Default for InMemoryTenantRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TenantRepository for InMemoryTenantRepository {
    async fn create(&self, tenant: Tenant) -> Result<Tenant, AdapterError> {
        self.store.insert(tenant.id, tenant.clone());
        Ok(tenant)
    }

    async fn get(&self, id: Uuid) -> Result<Option<Tenant>, AdapterError> {
        Ok(self.store.get(&id).map(|r| r.clone()))
    }

    async fn update(&self, tenant: Tenant) -> Result<Tenant, AdapterError> {
        if !self.store.contains_key(&tenant.id) {
            return Err(AdapterError::TenantNotFound(tenant.id.to_string()));
        }
        self.store.insert(tenant.id, tenant.clone());
        Ok(tenant)
    }

    async fn list_active(&self) -> Result<Vec<Tenant>, AdapterError> {
        Ok(self
            .store
            .iter()
            .filter(|r| r.is_active)
            .map(|r| r.clone())
            .collect())
    }
}

// ============================================================================
// Simple in-memory Cache
// ============================================================================

pub struct InMemoryCache {
    store: DashMap<String, (String, std::time::Instant, u64)>,
}

impl InMemoryCache {
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CachePort for InMemoryCache {
    async fn get(&self, key: &str) -> Result<Option<String>, AdapterError> {
        if let Some(entry) = self.store.get(key) {
            let (value, inserted, ttl) = entry.value();
            if inserted.elapsed().as_secs() < *ttl {
                return Ok(Some(value.clone()));
            }
            // expired
            drop(entry);
            self.store.remove(key);
        }
        Ok(None)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), AdapterError> {
        self.store
            .insert(key.to_string(), (value.to_string(), std::time::Instant::now(), ttl_secs));
        Ok(())
    }

    async fn invalidate(&self, key: &str) -> Result<(), AdapterError> {
        self.store.remove(key);
        Ok(())
    }
}

// ============================================================================
// Mock / Simulated Substrate Blockchain Adapter
// (In production replace with real subxt / jsonrpc client)
// ============================================================================

pub struct SubstrateAdapter {
    /// Simulated connection status
    connected: Arc<RwLock<bool>>,
    /// Simulated block number
    block_number: Arc<RwLock<u64>>,
    /// Chain name
    chain: String,
    /// Concurrency limiter for calls
    semaphore: Arc<Semaphore>,
    /// Simulated contract storage
    storage: DashMap<String, serde_json::Value>,
}

impl SubstrateAdapter {
    pub fn new(chain: impl Into<String>, max_concurrent: usize) -> Self {
        Self {
            connected: Arc::new(RwLock::new(true)),
            block_number: Arc::new(RwLock::new(1)),
            chain: chain.into(),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            storage: DashMap::new(),
        }
    }

    /// Simulate network latency / work
    async fn simulate_work(&self) {
        tokio::time::sleep(Duration::from_millis(20 + (rand_simple() % 50))).await;
        // advance block occasionally
        if rand_simple() % 5 == 0 {
            let mut bn = self.block_number.write();
            *bn += 1;
        }
    }
}

fn rand_simple() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64
}

#[async_trait]
impl BlockchainPort for SubstrateAdapter {
    async fn get_status(&self) -> Result<NodeStatus, AdapterError> {
        self.simulate_work().await;
        Ok(NodeStatus {
            connected: *self.connected.read(),
            chain: self.chain.clone(),
            block_number: *self.block_number.read(),
            peers: 4,
            last_checked: Utc::now(),
        })
    }

    async fn call_contract(
        &self,
        request: ContractCallRequest,
    ) -> Result<ContractCallResult, AdapterError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AdapterError::Internal("semaphore closed".into()))?;

        self.simulate_work().await;

        if !*self.connected.read() {
            return Err(AdapterError::Connection("node disconnected".into()));
        }

        // Very simple simulation of contract methods
        let result_data = match request.method.as_str() {
            "get" => {
                let key = format!("{}:value", request.contract_address);
                let val = self.storage.get(&key).map(|v| v.clone()).unwrap_or(serde_json::json!(0));
                Some(val)
            }
            "set" => {
                let new_val = request.args.get("new_value").cloned().unwrap_or(serde_json::json!(0));
                let key = format!("{}:value", request.contract_address);
                self.storage.insert(key, new_val.clone());
                Some(serde_json::json!({"ok": true, "value": new_val}))
            }
            "balance_of" => {
                let account = request.args.get("account").and_then(|v| v.as_str()).unwrap_or("default");
                let key = format!("{}:balance:{}", request.contract_address, account);
                let bal = self.storage.get(&key).map(|v| v.clone()).unwrap_or(serde_json::json!(0));
                Some(bal)
            }
            "mint" | "transfer" => {
                Some(serde_json::json!({"ok": true}))
            }
            _ => {
                return Ok(ContractCallResult {
                    request_id: Uuid::new_v4(),
                    success: false,
                    data: None,
                    error: Some(format!("unknown method: {}", request.method)),
                    gas_used: None,
                    block_hash: None,
                    timestamp: Utc::now(),
                });
            }
        };

        Ok(ContractCallResult {
            request_id: Uuid::new_v4(),
            success: true,
            data: result_data,
            error: None,
            gas_used: Some(25_000 + (rand_simple() % 10_000)),
            block_hash: Some(format!("0x{:016x}", rand_simple())),
            timestamp: Utc::now(),
        })
    }

    async fn deploy_contract(&self, request: DeployRequest) -> Result<String, AdapterError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AdapterError::Internal("semaphore closed".into()))?;

        self.simulate_work().await;

        // Simulate SS58-like address generation
        let address = format!(
            "5{}",
            Uuid::new_v4().to_string().replace('-', "")
        );

        // Init storage
        self.storage
            .insert(format!("{}:value", address), serde_json::json!(0));

        tracing::info!(
            tenant = %request.tenant_id,
            address = %address,
            "Contract deployed (simulated)"
        );

        Ok(address)
    }

    async fn get_storage(
        &self,
        contract: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>, AdapterError> {
        let full_key = format!("{}:{}", contract, key);
        Ok(self.storage.get(&full_key).map(|v| v.clone()))
    }
}

// ============================================================================
// No-op Event Publisher (can be replaced with Kafka/NATS/etc.)
// ============================================================================

pub struct LoggingEventPublisher;

#[async_trait]
impl EventPublisher for LoggingEventPublisher {
    async fn publish(&self, event_type: &str, payload: serde_json::Value) -> Result<(), AdapterError> {
        tracing::info!(event_type = event_type, payload = %payload, "event published");
        Ok(())
    }
}
