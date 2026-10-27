//! Decorator: wraps any BlockchainPort with billing checks.
//! SaasService stays unchanged — inject this instead of raw blockchain adapter.

use async_trait::async_trait;
use std::sync::Arc;

use crate::domain::*;
use crate::error::AdapterError;
use crate::ports::{BillingPort, BlockchainPort, TenantRepository};

pub struct BillingBlockchainProxy {
    inner: Arc<dyn BlockchainPort>,
    billing: Arc<dyn BillingPort>,
    tenants: Arc<dyn TenantRepository>,
}

impl BillingBlockchainProxy {
    pub fn new(
        inner: Arc<dyn BlockchainPort>,
        billing: Arc<dyn BillingPort>,
        tenants: Arc<dyn TenantRepository>,
    ) -> Self {
        Self {
            inner,
            billing,
            tenants,
        }
    }
}

#[async_trait]
impl BlockchainPort for BillingBlockchainProxy {
    async fn get_status(&self) -> Result<NodeStatus, AdapterError> {
        self.inner.get_status().await
    }

    async fn call_contract(
        &self,
        request: ContractCallRequest,
    ) -> Result<ContractCallResult, AdapterError> {
        let tenant = self
            .tenants
            .get(request.tenant_id)
            .await?
            .ok_or_else(|| AdapterError::TenantNotFound(request.tenant_id.to_string()))?;

        self.billing
            .check_and_reserve(request.tenant_id, &tenant.plan)
            .await?;

        let result = self.inner.call_contract(request.clone()).await?;

        let gas = result.gas_used.unwrap_or(0);
        let _ = self.billing.record_usage(request.tenant_id, gas).await;

        Ok(result)
    }

    async fn deploy_contract(&self, request: DeployRequest) -> Result<String, AdapterError> {
        let tenant = self
            .tenants
            .get(request.tenant_id)
            .await?
            .ok_or_else(|| AdapterError::TenantNotFound(request.tenant_id.to_string()))?;

        self.billing
            .check_and_reserve(request.tenant_id, &tenant.plan)
            .await?;

        let addr = self.inner.deploy_contract(request.clone()).await?;
        let _ = self.billing.record_usage(request.tenant_id, 100_000).await;
        Ok(addr)
    }

    async fn get_storage(
        &self,
        contract: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>, AdapterError> {
        self.inner.get_storage(contract, key).await
    }
}
