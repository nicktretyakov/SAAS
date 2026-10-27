//! CachePort on Redis
//! Enable with feature `redis-cache`.

#![cfg(feature = "redis-cache")]

use async_trait::async_trait;
use redis::AsyncCommands;

use crate::error::AdapterError;
use crate::ports::CachePort;

pub struct RedisCache {
    conn: redis::aio::ConnectionManager,
}

impl RedisCache {
    pub async fn connect(redis_url: &str) -> Result<Self, AdapterError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| AdapterError::Cache(format!("redis open: {}", e)))?;
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| AdapterError::Cache(format!("redis connect: {}", e)))?;
        Ok(Self { conn })
    }
}

#[async_trait]
impl CachePort for RedisCache {
    async fn get(&self, key: &str) -> Result<Option<String>, AdapterError> {
        let mut conn = self.conn.clone();
        let val: Option<String> = conn
            .get(key)
            .await
            .map_err(|e| AdapterError::Cache(e.to_string()))?;
        Ok(val)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), AdapterError> {
        let mut conn = self.conn.clone();
        conn.set_ex(key, value, ttl_secs)
            .await
            .map_err(|e| AdapterError::Cache(e.to_string()))?;
        Ok(())
    }

    async fn invalidate(&self, key: &str) -> Result<(), AdapterError> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .del(key)
            .await
            .map_err(|e| AdapterError::Cache(e.to_string()))?;
        Ok(())
    }
}
