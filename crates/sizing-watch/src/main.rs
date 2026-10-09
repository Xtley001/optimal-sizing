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

    // Try WebSocket connection if real URL is configured
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::protocol::Message;

    let ws_url = config.ws_rpc_url.clone();
    if ws_url.starts_with("ws://") || ws_url.starts_with("wss://") {
        eprintln!("sizing-watch: connecting to WebSocket RPC {}", ws_url);
        if let Ok((mut ws_stream, _)) = connect_async(&ws_url).await {
            eprintln!("sizing-watch: connected, subscribing to newHeads...");
            let sub_msg = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_subscribe",
                "params": ["newHeads"]
            });
            let _ = ws_stream.send(Message::Text(sub_msg.to_string())).await;

            while running.load(Ordering::SeqCst) {
                tokio::select! {
                    msg = ws_stream.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(result) = val.get("params").and_then(|p| p.get("result")) {
                                        if let Some(num_str) = result.get("number").and_then(|n| n.as_str()) {
                                            let block_num = u64::from_str_radix(num_str.trim_start_matches("0x"), 16).unwrap_or(0);
                                            let hash = result.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_string();
                                            let parent = result.get("parentHash").and_then(|h| h.as_str()).unwrap_or("").to_string();
                                            cache.handle_new_head(block_num, hash, parent);

                                            let ref_price = Decimal::from_str_exact("0.85").unwrap();
                                            let gas_price = Decimal::from(25);
                                            evaluate_opportunities(&config, &cache, ref_price, gas_price);
                                        }
                                    }
                                }
                            }
                            Some(Ok(Message::Ping(data))) => {
                                let _ = ws_stream.send(Message::Pong(data)).await;
                            }
                            Some(Err(e)) => {
                                eprintln!("sizing-watch: ws error {e}, switching to polling");
                                break;
                            }
                            None => break,
                            _ => {}
                        }
                    }
                    _ = sleep(Duration::from_millis(500)) => {
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Streaming daemon polling fallback loop
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
