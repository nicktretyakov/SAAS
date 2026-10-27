//! In-memory adapters (default, no external deps)

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

// ---------------------------------------------------------------------------
// Tenant repository
// ---------------------------------------------------------------------------

pub struct InMemoryTenantRepository {
    store: DashMap<Uuid, Tenant>,
    /// api_key_hash -> tenant_id
    by_key: DashMap<String, Uuid>,
}

impl InMemoryTenantRepository {
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
            by_key: DashMap::new(),
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
        if let Some(ref hash) = tenant.api_key_hash {
            self.by_key.insert(hash.clone(), tenant.id);
        }
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
        if let Some(ref hash) = tenant.api_key_hash {
            self.by_key.insert(hash.clone(), tenant.id);
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

    async fn get_by_api_key_hash(&self, key_hash: &str) -> Result<Option<Tenant>, AdapterError> {
        if let Some(id) = self.by_key.get(key_hash) {
            return self.get(*id).await;
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Simulated Substrate
// ---------------------------------------------------------------------------

pub struct SubstrateAdapter {
    connected: Arc<RwLock<bool>>,
    block_number: Arc<RwLock<u64>>,
    chain: String,
    semaphore: Arc<Semaphore>,
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

    async fn simulate_work(&self) {
        tokio::time::sleep(Duration::from_millis(20 + (rand_simple() % 50))).await;
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

        let result_data = match request.method.as_str() {
            "get" => {
                let key = format!("{}:value", request.contract_address);
                let val = self
                    .storage
                    .get(&key)
                    .map(|v| v.clone())
                    .unwrap_or(serde_json::json!(0));
                Some(val)
            }
            "set" => {
                let new_val = request
                    .args
                    .get("new_value")
                    .cloned()
                    .unwrap_or(serde_json::json!(0));
                let key = format!("{}:value", request.contract_address);
                self.storage.insert(key, new_val.clone());
                Some(serde_json::json!({"ok": true, "value": new_val}))
            }
            "balance_of" => {
                let account = request
                    .args
                    .get("account")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let key = format!("{}:balance:{}", request.contract_address, account);
                let bal = self
                    .storage
                    .get(&key)
                    .map(|v| v.clone())
                    .unwrap_or(serde_json::json!(0));
                Some(bal)
            }
            "mint" | "transfer" => Some(serde_json::json!({"ok": true})),
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

        let address = format!("5{}", Uuid::new_v4().to_string().replace('-', ""));

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

// ---------------------------------------------------------------------------
// Logging events
// ---------------------------------------------------------------------------

pub struct LoggingEventPublisher;

#[async_trait]
impl EventPublisher for LoggingEventPublisher {
    async fn publish(&self, event_type: &str, payload: serde_json::Value) -> Result<(), AdapterError> {
        tracing::info!(event_type = event_type, payload = %payload, "event published");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// In-memory billing
// ---------------------------------------------------------------------------

pub struct InMemoryBilling {
    /// tenant_id -> (day, calls, gas)
    usage: DashMap<Uuid, (String, u64, u64)>,
    /// per-tenant concurrency limiters
    semaphores: DashMap<Uuid, Arc<Semaphore>>,
}

impl InMemoryBilling {
    pub fn new() -> Self {
        Self {
            usage: DashMap::new(),
            semaphores: DashMap::new(),
        }
    }

    fn today() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }
}

impl Default for InMemoryBilling {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BillingPort for InMemoryBilling {
    async fn check_and_reserve(&self, tenant_id: Uuid, plan: &TenantPlan) -> Result<(), AdapterError> {
        let day = Self::today();
        let mut entry = self.usage.entry(tenant_id).or_insert_with(|| (day.clone(), 0, 0));
        if entry.0 != day {
            *entry = (day, 0, 0);
        }
        if entry.1 >= plan.daily_call_limit {
            return Err(AdapterError::QuotaExceeded(format!(
                "daily call limit {} reached",
                plan.daily_call_limit
            )));
        }
        if entry.2 >= plan.daily_gas_limit {
            return Err(AdapterError::QuotaExceeded(format!(
                "daily gas limit {} reached",
                plan.daily_gas_limit
            )));
        }
        // Ensure semaphore exists for concurrency
        self.semaphores
            .entry(tenant_id)
            .or_insert_with(|| Arc::new(Semaphore::new(plan.max_concurrency as usize)));
        Ok(())
    }

    async fn record_usage(&self, tenant_id: Uuid, gas_used: u64) -> Result<(), AdapterError> {
        let day = Self::today();
        let mut entry = self.usage.entry(tenant_id).or_insert_with(|| (day.clone(), 0, 0));
        if entry.0 != day {
            *entry = (day, 0, 0);
        }
        entry.1 += 1;
        entry.2 += gas_used;
        Ok(())
    }

    async fn get_usage(&self, tenant_id: Uuid) -> Result<UsageRecord, AdapterError> {
        let day = Self::today();
        if let Some(entry) = self.usage.get(&tenant_id) {
            if entry.0 == day {
                return Ok(UsageRecord {
                    tenant_id,
                    calls: entry.1,
                    gas_used: entry.2,
                    day,
                });
            }
        }
        Ok(UsageRecord {
            tenant_id,
            calls: 0,
            gas_used: 0,
            day,
        })
    }
}
