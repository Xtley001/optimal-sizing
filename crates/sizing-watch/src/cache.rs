//! In-memory state machine and caching for `sizing-watch`.
//! Per `docs/STREAMING_SPEC.md § 3`.

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Synchronized in-memory pool state.
#[derive(Debug, Clone)]
pub struct PoolState {
    /// Pool identifier.
    pub pool_id: String,
    /// Latest block number when state was updated.
    pub last_block_number: u64,
    /// Current token reserves.
    pub reserves: Vec<Decimal>,
    /// Sqrt price current (for CLMM).
    pub sqrt_price_current: Option<Decimal>,
    /// Update timestamp in epoch milliseconds.
    pub timestamp_ms: u64,
}

/// Cache status indicating synchronization health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStatus {
    /// Fully synchronized with current chain head.
    #[default]
    Synchronized,
    /// Stale due to missed blocks or reorg.
    Stale,
}

/// Thread-safe in-memory cache for watched pools.
#[derive(Debug, Clone, Default)]
pub struct PoolCache {
    states: Arc<RwLock<HashMap<String, PoolState>>>,
    last_block_number: Arc<RwLock<u64>>,
    last_block_hash: Arc<RwLock<String>>,
    status: Arc<RwLock<SyncStatus>>,
}

impl PoolCache {
    /// Creates a new empty `PoolCache`.
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            last_block_number: Arc::new(RwLock::new(0)),
            last_block_hash: Arc::new(RwLock::new(String::new())),
            status: Arc::new(RwLock::new(SyncStatus::Synchronized)),
        }
    }

    /// Updates or inserts a pool state.
    pub fn update_pool(&self, state: PoolState) {
        let mut map = self.states.write().unwrap();
        map.insert(state.pool_id.clone(), state);
    }

    /// Retrieves a cloned snapshot of a pool's state.
    pub fn get_pool(&self, pool_id: &str) -> Option<PoolState> {
        let map = self.states.read().unwrap();
        map.get(pool_id).cloned()
    }

    /// Handles a new block header event, verifying sequentiality and reorg detection.
    pub fn handle_new_head(
        &self,
        block_number: u64,
        block_hash: String,
        parent_hash: String,
    ) -> bool {
        let mut last_num = self.last_block_number.write().unwrap();
        let mut last_hash = self.last_block_hash.write().unwrap();
        let mut status = self.status.write().unwrap();

        if *last_num == 0 {
            *last_num = block_number;
            *last_hash = block_hash;
            *status = SyncStatus::Synchronized;
            return true;
        }

        // Sequential check
        if block_number == *last_num + 1 {
            if !last_hash.is_empty() && parent_hash != *last_hash {
                // Reorg detected!
                *status = SyncStatus::Stale;
                *last_num = block_number;
                *last_hash = block_hash;
                return false;
            }
            *last_num = block_number;
            *last_hash = block_hash;
            *status = SyncStatus::Synchronized;
            true
        } else if block_number > *last_num + 1 {
            // Gap detected!
            *status = SyncStatus::Stale;
            *last_num = block_number;
            *last_hash = block_hash;
            false
        } else {
            // Out-of-order or duplicate block
            false
        }
    }

    /// Returns current synchronization status.
    pub fn sync_status(&self) -> SyncStatus {
        *self.status.read().unwrap()
    }

    /// Latest known block number.
    pub fn latest_block(&self) -> u64 {
        *self.last_block_number.read().unwrap()
    }
}
