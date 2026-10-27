//! BlockchainPort via jsonrpsee (Substrate WS/HTTP RPC)
//!
//! Enable with feature `substrate-rpc`.
//! For full typed Ink! calls prefer subxt; this adapter uses low-level
//! `state_call` / `contracts_*` style RPC and is enough to wire SaaS → node.

#![cfg(feature = "substrate-rpc")]

use async_trait::async_trait;
use chrono::Utc;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::domain::*;
use crate::error::AdapterError;
use crate::ports::BlockchainPort;

pub struct SubstrateRpcAdapter {
    client: Arc<WsClient>,
    chain: String,
    semaphore: Arc<Semaphore>,
}

impl SubstrateRpcAdapter {
    /// Connect to a Substrate node WS endpoint, e.g. `ws://127.0.0.1:9944`
    pub async fn connect(url: &str, chain: impl Into<String>, max_concurrent: usize) -> Result<Self, AdapterError> {
        let client = WsClientBuilder::default()
            .build(url)
            .await
            .map_err(|e| AdapterError::Connection(e.to_string()))?;
        Ok(Self {
            client: Arc::new(client),
            chain: chain.into(),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        })
    }
}

#[async_trait]
impl BlockchainPort for SubstrateRpcAdapter {
    async fn get_status(&self) -> Result<NodeStatus, AdapterError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AdapterError::Internal("semaphore closed".into()))?;

        // system_health / chain_getHeader — standard Substrate RPC
        let peers: u32 = self
            .client
            .request("system_health", rpc_params![])
            .await
            .map(|h: serde_json::Value| {
                h.get("peers")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32
            })
            .unwrap_or(0);

        let header: serde_json::Value = self
            .client
            .request("chain_getHeader", rpc_params![])
            .await
            .map_err(|e| AdapterError::Connection(e.to_string()))?;

        let block_number = header
            .get("number")
            .and_then(|v| v.as_str())
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0);

        Ok(NodeStatus {
            connected: true,
            chain: self.chain.clone(),
            block_number,
            peers,
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

        // contracts_call (dry-run) — pallet-contracts RPC
        // Production: encode Ink! message with proper scale codec / subxt metadata.
        let params = serde_json::json!({
            "origin": request.caller,
            "dest": request.contract_address,
            "value": 0,
            "gasLimit": null,
            "storageDepositLimit": null,
            "inputData": request.args,
        });

        let result: Result<serde_json::Value, _> = self
            .client
            .request("contracts_call", rpc_params![params])
            .await;

        match result {
            Ok(data) => Ok(ContractCallResult {
                request_id: Uuid::new_v4(),
                success: true,
                data: Some(data),
                error: None,
                gas_used: Some(0),
                block_hash: None,
                timestamp: Utc::now(),
            }),
            Err(e) => Ok(ContractCallResult {
                request_id: Uuid::new_v4(),
                success: false,
                data: None,
                error: Some(e.to_string()),
                gas_used: None,
                block_hash: None,
                timestamp: Utc::now(),
            }),
        }
    }

    async fn deploy_contract(&self, request: DeployRequest) -> Result<String, AdapterError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AdapterError::Internal("semaphore closed".into()))?;

        // Real deploy: upload code + instantiate via contracts API / extrinsic.
        // Placeholder returns deterministic-looking address; wire subxt extrinsic here.
        Err(AdapterError::Deployment(format!(
            "deploy via RPC not fully wired; use cargo-contract or subxt extrinsic \
             (tenant={}, wasm={})",
            request.tenant_id, request.wasm_path
        )))
    }

    async fn get_storage(
        &self,
        contract: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>, AdapterError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AdapterError::Internal("semaphore closed".into()))?;

        // state_getStorage with contracts storage key prefix (simplified)
        let result: Option<String> = self
            .client
            .request(
                "state_getStorage",
                rpc_params![format!("{}:{}", contract, key)],
            )
            .await
            .map_err(|e| AdapterError::Connection(e.to_string()))?;

        Ok(result.map(|hex| serde_json::json!({ "raw": hex })))
    }
}
