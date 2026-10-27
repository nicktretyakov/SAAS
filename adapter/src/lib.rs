//! SaaS Adapter Layer
//!
//! SOLID + multi-threaded ports & adapters for Substrate SaaS.
//!
//! # Features
//! - `memory` (default): in-memory tenants, cache, simulated chain, billing
//! - `postgres`: SQLx TenantRepository
//! - `redis-cache`: Redis CachePort
//! - `nats`: NATS EventPublisher
//! - `substrate-rpc`: jsonrpsee BlockchainPort
//! - `full`: all of the above
//!
//! SaasService depends only on traits — swap adapters without changing it.
//! Billing is applied via `BillingBlockchainProxy` decorator.

pub mod auth;
pub mod domain;
pub mod error;
pub mod ports;
pub mod services;
pub mod adapters;

pub use auth::*;
pub use domain::*;
pub use error::*;
pub use ports::*;
pub use services::*;
pub use adapters::*;
