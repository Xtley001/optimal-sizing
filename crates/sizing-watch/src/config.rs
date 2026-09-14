//! Configuration parsing for `sizing-watch`.
//! Per `docs/STREAMING_SPEC.md § 2`.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Daemon configuration loaded from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// WebSocket RPC endpoint URL.
    pub ws_rpc_url: String,
    /// Reference price oracle WebSocket URL (optional).
    #[serde(default)]
    pub oracle_ws_url: Option<String>,
    /// Minimum expected profit threshold in USD to trigger opportunity output.
    pub min_profit_usd: Decimal,
    /// Default fixed execution cost in gas units.
    #[serde(default = "default_gas_units")]
    pub default_fixed_cost_gas_units: u64,
    /// Watched liquidity pools.
    pub pools: Vec<PoolConfig>,
    /// Multi-hop routes to monitor.
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
}

fn default_gas_units() -> u64 {
    150_000
}

/// Configuration for an individual watched pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Unique pool identifier.
    pub id: String,
    /// Contract address on-chain.
    pub address: String,
    /// Curve type: `cpmm`, `stableswap`, `clmm`, `balancer`, `dodo`, `cryptoswap`.
    pub curve_type: String,
    /// Fee retention fraction (e.g. `0.997` for 30bps fee).
    pub fee_retention: Decimal,
    /// Amplification parameter (for StableSwap / CryptoSwap).
    #[serde(default)]
    pub amplification: Option<Decimal>,
    /// Gamma smoothing parameter (for CryptoSwap).
    #[serde(default)]
    pub gamma: Option<Decimal>,
    /// Token weights (for Balancer).
    #[serde(default)]
    pub weights: Option<Vec<Decimal>>,
    /// Input token index (default 0).
    #[serde(default)]
    pub input_token_index: usize,
    /// Output token index (default 1).
    #[serde(default = "default_one")]
    pub output_token_index: usize,
}

fn default_one() -> usize {
    1
}

/// Configuration for a multi-hop route composed of watched pool IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    /// Unique route identifier.
    pub id: String,
    /// Ordered list of pool IDs comprising the route.
    pub legs: Vec<String>,
}

impl WatchConfig {
    /// Loads configuration from a JSON file.
    pub fn load_from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }
}
