//! `sizing-watch` — Real-time WebSocket streaming daemon for optimal sizing opportunities.
//! Per `docs/STREAMING_SPEC.md`.

pub mod cache;
pub mod config;
pub mod engine;

pub use cache::{PoolCache, PoolState, SyncStatus};
pub use config::{PoolConfig, RouteConfig, WatchConfig};
pub use engine::{evaluate_opportunities, OpportunityEvent};
