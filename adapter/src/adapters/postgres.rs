//! TenantRepository on PostgreSQL via SQLx
//! Enable with feature `postgres`.

#![cfg(feature = "postgres")]

use async_trait::async_trait;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::*;
use crate::error::AdapterError;
use crate::ports::TenantRepository;

pub struct PostgresTenantRepository {
    pool: PgPool,
}

impl PostgresTenantRepository {
    pub async fn connect(database_url: &str) -> Result<Self, AdapterError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .connect(database_url)
            .await
            .map_err(|e| AdapterError::Internal(format!("postgres connect: {}", e)))?;

        // Schema migration (idempotent)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tenants (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                contract_address TEXT,
                created_at TIMESTAMPTZ NOT NULL,
                is_active BOOLEAN NOT NULL DEFAULT true,
                api_key_hash TEXT UNIQUE,
                daily_call_limit BIGINT NOT NULL DEFAULT 10000,
                daily_gas_limit BIGINT NOT NULL DEFAULT 50000000,
                max_concurrency INT NOT NULL DEFAULT 16
            );
            CREATE INDEX IF NOT EXISTS idx_tenants_api_key ON tenants(api_key_hash);
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| AdapterError::Internal(format!("migrate: {}", e)))?;

        Ok(Self { pool })
    }

    fn row_to_tenant(row: sqlx::postgres::PgRow) -> Tenant {
        Tenant {
            id: row.get("id"),
            name: row.get("name"),
            contract_address: row.get("contract_address"),
            created_at: row.get("created_at"),
            is_active: row.get("is_active"),
            api_key_hash: row.get("api_key_hash"),
            plan: TenantPlan {
                daily_call_limit: row.get::<i64, _>("daily_call_limit") as u64,
                daily_gas_limit: row.get::<i64, _>("daily_gas_limit") as u64,
                max_concurrency: row.get::<i32, _>("max_concurrency") as u32,
            },
        }
    }
}

#[async_trait]
impl TenantRepository for PostgresTenantRepository {
    async fn create(&self, tenant: Tenant) -> Result<Tenant, AdapterError> {
        sqlx::query(
            r#"
            INSERT INTO tenants
              (id, name, contract_address, created_at, is_active, api_key_hash,
               daily_call_limit, daily_gas_limit, max_concurrency)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            "#,
        )
        .bind(tenant.id)
        .bind(&tenant.name)
        .bind(&tenant.contract_address)
        .bind(tenant.created_at)
        .bind(tenant.is_active)
        .bind(&tenant.api_key_hash)
        .bind(tenant.plan.daily_call_limit as i64)
        .bind(tenant.plan.daily_gas_limit as i64)
        .bind(tenant.plan.max_concurrency as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| AdapterError::Internal(format!("insert tenant: {}", e)))?;
        Ok(tenant)
    }

    async fn get(&self, id: Uuid) -> Result<Option<Tenant>, AdapterError> {
        let row = sqlx::query("SELECT * FROM tenants WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AdapterError::Internal(format!("get tenant: {}", e)))?;
        Ok(row.map(Self::row_to_tenant))
    }

    async fn update(&self, tenant: Tenant) -> Result<Tenant, AdapterError> {
        let r = sqlx::query(
            r#"
            UPDATE tenants SET
              name=$2, contract_address=$3, is_active=$4, api_key_hash=$5,
              daily_call_limit=$6, daily_gas_limit=$7, max_concurrency=$8
            WHERE id=$1
            "#,
        )
        .bind(tenant.id)
        .bind(&tenant.name)
        .bind(&tenant.contract_address)
        .bind(tenant.is_active)
        .bind(&tenant.api_key_hash)
        .bind(tenant.plan.daily_call_limit as i64)
        .bind(tenant.plan.daily_gas_limit as i64)
        .bind(tenant.plan.max_concurrency as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| AdapterError::Internal(format!("update tenant: {}", e)))?;

        if r.rows_affected() == 0 {
            return Err(AdapterError::TenantNotFound(tenant.id.to_string()));
        }
        Ok(tenant)
    }

    async fn list_active(&self) -> Result<Vec<Tenant>, AdapterError> {
        let rows = sqlx::query("SELECT * FROM tenants WHERE is_active = true ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AdapterError::Internal(format!("list tenants: {}", e)))?;
        Ok(rows.into_iter().map(Self::row_to_tenant).collect())
    }

    async fn get_by_api_key_hash(&self, key_hash: &str) -> Result<Option<Tenant>, AdapterError> {
        let row = sqlx::query("SELECT * FROM tenants WHERE api_key_hash = $1")
            .bind(key_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AdapterError::Internal(format!("get by key: {}", e)))?;
        Ok(row.map(Self::row_to_tenant))
    }
}
