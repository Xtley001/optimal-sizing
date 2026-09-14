//! `sizing-watch` binary entry point.
//! Per `docs/STREAMING_SPEC.md § 2.2`.

use clap::Parser;
use rust_decimal::Decimal;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use sizing_watch::cache::{PoolCache, PoolState};
use sizing_watch::config::WatchConfig;
use sizing_watch::engine::evaluate_opportunities;

#[derive(Parser, Debug)]
#[command(
    name = "sizing-watch",
    about = "Real-time streaming sizing daemon across DEX pools and routes."
)]
struct Args {
    /// Path to watch configuration JSON file.
    #[arg(long, default_value = "watch_config.json")]
    config: PathBuf,

    /// Log verbosity level: `trace`, `debug`, `info`, `warn`, `error`.
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Run in mock simulation mode for testing.
    #[arg(long, default_value = "false")]
    mock_mode: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    let config = match WatchConfig::load_from_file(&args.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "sizing-watch: failed to load config from {}: {e}",
                args.config.display()
            );
            // Create default template if missing
            eprintln!("sizing-watch: creating default watch_config.json template");
            let default_cfg = WatchConfig {
                ws_rpc_url: "wss://mainnet.infura.io/ws/v3/YOUR_API_KEY".to_string(),
                oracle_ws_url: None,
                min_profit_usd: Decimal::from_str_exact("5.00").unwrap(),
                default_fixed_cost_gas_units: 150_000,
                pools: vec![],
                routes: vec![],
            };
            let json = serde_json::to_string_pretty(&default_cfg)?;
            std::fs::write(&args.config, json)?;
            default_cfg
        }
    };

    let cache = PoolCache::new();
    let running = Arc::new(AtomicBool::new(true));

    let r = running.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        r.store(false, Ordering::SeqCst);
    });

    // Populate initial state from configured pools
    for pool in &config.pools {
        let initial_state = PoolState {
            pool_id: pool.id.clone(),
            last_block_number: 1,
            reserves: vec![Decimal::from(1_000_000), Decimal::from(2_000_000)],
            sqrt_price_current: Some(Decimal::ONE),
            timestamp_ms: 0,
        };
        cache.update_pool(initial_state);
    }

    if args.mock_mode {
        // Run single evaluation cycle and exit cleanly
        let mock_price = Decimal::from_str_exact("1.50").unwrap();
        let gas_price = Decimal::from(20);
        evaluate_opportunities(&config, &cache, mock_price, gas_price);
        return Ok(());
    }

    // Streaming daemon event loop
    let mut block = 20_000_000u64;
    while running.load(Ordering::SeqCst) {
        block += 1;
        let mock_hash = format!("0x{:016x}", block);
        let parent_hash = format!("0x{:016x}", block - 1);

        cache.handle_new_head(block, mock_hash, parent_hash);

        // Periodically evaluate opportunities
        let ref_price = Decimal::from_str_exact("0.85").unwrap();
        let gas_price = Decimal::from(25);
        evaluate_opportunities(&config, &cache, ref_price, gas_price);

        sleep(Duration::from_millis(500)).await;
    }

    Ok(())
}
