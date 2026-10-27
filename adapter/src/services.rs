//! Application services — orchestrate ports (Dependency Inversion)
//! Intentionally free of HTTP / Postgres / Redis / NATS details.

use std::sync::Arc;
use uuid::Uuid;

use crate::auth::{generate_api_key, hash_api_key};
use crate::domain::*;
use crate::error::AdapterError;
use crate::ports::*;

/// Result of tenant registration including one-time raw API key
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegisterResult {
    pub tenant: Tenant,
    /// Shown only once — store hash in DB
    pub api_key: String,
}

pub struct SaasService {
    blockchain: Arc<dyn BlockchainPort>,
    tenants: Arc<dyn TenantRepository>,
    cache: Arc<dyn CachePort>,
    events: Arc<dyn EventPublisher>,
}

impl SaasService {
    pub fn new(
        blockchain: Arc<dyn BlockchainPort>,
        tenants: Arc<dyn TenantRepository>,
        cache: Arc<dyn CachePort>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            blockchain,
            tenants,
            cache,
            events,
        }
    }

    pub fn tenants(&self) -> Arc<dyn TenantRepository> {
        self.tenants.clone()
    }

    /// Register a new tenant and issue an API key
    pub async fn register_tenant(&self, name: String) -> Result<RegisterResult, AdapterError> {
        if name.trim().is_empty() {
            return Err(AdapterError::Validation("name cannot be empty".into()));
        }

        let (raw_key, key_hash) = generate_api_key();

        let tenant = Tenant {
            id: Uuid::new_v4(),
            name,
            contract_address: None,
            created_at: chrono::Utc::now(),
            is_active: true,
            api_key_hash: Some(key_hash),
            plan: TenantPlan::default(),
        };

        let created = self.tenants.create(tenant).await?;
        self.events
            .publish(
                "tenant.registered",
                serde_json::json!({ "id": created.id, "name": created.name }),
            )
            .await?;

        Ok(RegisterResult {
            tenant: created,
            api_key: raw_key,
        })
    }

    pub async fn deploy_for_tenant(
        &self,
        tenant_id: Uuid,
        wasm_path: String,
        constructor: String,
        args: serde_json::Value,
    ) -> Result<Tenant, AdapterError> {
        let mut tenant = self
            .tenants
            .get(tenant_id)
            .await?
            .ok_or_else(|| AdapterError::TenantNotFound(tenant_id.to_string()))?;

        let address = self
            .blockchain
            .deploy_contract(DeployRequest {
                tenant_id,
                wasm_path,
                constructor,
                args,
            })
            .await?;

        tenant.contract_address = Some(address.clone());
        let updated = self.tenants.update(tenant).await?;

        self.events
            .publish(
                "contract.deployed",
                serde_json::json!({
                    "tenant_id": tenant_id,
                    "address": address
                }),
            )
            .await?;

        Ok(updated)
    }

    pub async fn call(
        &self,
        request: ContractCallRequest,
    ) -> Result<ContractCallResult, AdapterError> {
        let _tenant = self
            .tenants
            .get(request.tenant_id)
            .await?
            .ok_or_else(|| AdapterError::TenantNotFound(request.tenant_id.to_string()))?;

        let is_read = matches!(
            request.method.as_str(),
            "get" | "balance_of" | "total_supply" | "owner"
        );
        let contract_addr = request.contract_address.clone();
        let cache_key = if is_read {
            Some(format!(
                "call:{}:{}:{}",
                request.contract_address,
                request.method,
                request.args.to_string()
            ))
        } else {
            None
        };

        if let Some(ref key) = cache_key {
            if let Ok(Some(cached)) = self.cache.get(key).await {
                if let Ok(parsed) = serde_json::from_str::<ContractCallResult>(&cached) {
                    return Ok(parsed);
                }
            }
        }

        let result = self.blockchain.call_contract(request).await?;

        if result.success {
            if let Some(ref key) = cache_key {
                let _ = self
                    .cache
                    .set(key, &serde_json::to_string(&result)?, 30)
                    .await;
            }
            if !is_read {
                let _ = self
                    .cache
                    .invalidate(&format!("call:{}:get:{{}}", contract_addr))
                    .await;
            }
        }

        self.events
            .publish(
                "contract.called",
                serde_json::json!({
                    "request_id": result.request_id,
                    "success": result.success
                }),
            )
            .await?;

        Ok(result)
    }

    pub async fn node_status(&self) -> Result<NodeStatus, AdapterError> {
        self.blockchain.get_status().await
    }

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>, AdapterError> {
        self.tenants.list_active().await
    }

    /// Rotate API key for a tenant
    pub async fn rotate_api_key(&self, tenant_id: Uuid) -> Result<String, AdapterError> {
        let mut tenant = self
            .tenants
            .get(tenant_id)
            .await?
            .ok_or_else(|| AdapterError::TenantNotFound(tenant_id.to_string()))?;
        let (raw, hash) = generate_api_key();
        tenant.api_key_hash = Some(hash);
        self.tenants.update(tenant).await?;
        Ok(raw)
    }

    pub fn verify_api_key_hash(raw: &str) -> String {
        hash_api_key(raw)
    }
}
