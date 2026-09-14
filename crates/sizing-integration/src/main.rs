//! Real, read-only integration against live Ethereum mainnet pool state
//! (Uniswap v2 USDC/WETH + Curve 3pool, addresses locked in
//! `docs/ARCHITECTURE.md § Reference integration`). Requires `RPC_URL` env
//! var, no fallback — fails loudly if unset, per this project's stated decision-logging discipline.
//!
//! This is the one crate in the workspace permitted to depend on
//! chain/network crates (`docs/ARCHITECTURE.md § Module boundaries`).
//! Function selectors below were computed via keccak256 (not transcribed
//! from memory) and cross-checked against Curve's own published docs
//! (<https://curve.readthedocs.io/exchange-pools.html>) for `balances`,
//! `A`, `fee` signatures.
//!
//! CANNOT BE VERIFIED END-TO-END IN THIS SANDBOX: the build environment's
//! network allowlist does not include any Ethereum RPC provider, so this
//! has been written carefully but not run against live state, and the
//! acceptance criterion "manually verified against actual pool state...
//! cross-check reserve numbers against a block explorer" (the original build plan
//! Session 7) has NOT been satisfied. Flagged in the Session 7 handoff.
//! This applies equally to the Multicall3 batching added below (`ROADMAP.md`
//! "Multicall batching") — `multicall.rs`'s ABI encode/decode has unit
//! test coverage against synthetic data, but the batched `eth_call`
//! against the real Multicall3 deployment has not been run live either.
//!
//! Multicall3 batching note: reads are now batched into 3 round-trips
//! total instead of 10 (Uniswap: getReserves+token0+token1 in one call,
//! then decimals(token0)+decimals(token1) in a second — decimals()
//! genuinely depends on the first call's result, so this is 2 round-trips
//! not 1; Curve 3pool: all 5 reads have no interdependency, so they batch
//! into a single round-trip). See `multicall.rs`'s module doc for why the
//! Uniswap side can't collapse further.
//!
//! Optional overrides (added per external review, all default to the
//! documented synthetic-demo behavior if unset):
//! - `CURVE_A_OVERRIDE`: use this exact amplification value instead of
//!   the unverified default interpretation of Curve 3pool's `A()`.
//! - `UNISWAP_REFERENCE_PRICE` / `CURVE_REFERENCE_PRICE`: use this exact
//!   price as `reference_price` instead of the canned 0.1% synthetic
//!   offset, for a real external signal.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::{Cpmm, StableSwap};
use sizing_core::{PricingCurve, SizingAlgorithm, SizingConstraints};
use std::env;

mod multicall;
use multicall::{Call3, MULTICALL3_ADDRESS};

const UNISWAP_V2_USDC_WETH_DEFAULT: &str = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc";
const CURVE_3POOL_DEFAULT: &str = "0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7";

// Selectors: first 4 bytes of keccak256(signature), computed directly
// (see module doc comment) rather than transcribed.
const SEL_GET_RESERVES: &str = "0902f1ac"; // getReserves()
const SEL_TOKEN0: &str = "0dfe1681"; // token0()
const SEL_TOKEN1: &str = "d21220a7"; // token1()
const SEL_DECIMALS: &str = "313ce567"; // decimals()
const SEL_BALANCES: &str = "4903b0d1"; // balances(uint256)
const SEL_A: &str = "f446c1d0"; // A()
const SEL_FEE: &str = "ddca3f43"; // fee()

fn rpc_url() -> String {
    // Never falls back to a hardcoded value — fails loudly if unset, per
    // ARCHITECTURE.md § Reference integration and this project's stated decision-logging discipline.
    env::var("RPC_URL").unwrap_or_else(|_| {
        eprintln!("RPC_URL environment variable is not set. sizing-integration requires a live Ethereum mainnet RPC endpoint and will not fall back to a hardcoded value.");
        std::process::exit(1);
    })
}

/// Minimal hex encode/decode — no extra dependency for this narrow need.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            write!(acc, "{b:02x}").unwrap();
            acc
        })
}

fn from_hex(s: &str) -> Vec<u8> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("invalid hex from RPC response"))
        .collect()
}

/// ABI-encodes a single uint256 argument, left-padded to 32 bytes.
fn encode_uint256_arg(value: u64) -> String {
    let mut bytes = [0u8; 32];
    bytes[24..32].copy_from_slice(&value.to_be_bytes());
    to_hex(&bytes)
}

/// Calls `eth_call` against RPC_URL for `to` with calldata `data` (hex,
/// no 0x prefix), returns the raw decoded result bytes.
fn eth_call(rpc_url: &str, to: &str, data: &str) -> Vec<u8> {
    let client = reqwest::blocking::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [{"to": to, "data": format!("0x{data}")}, "latest"]
    });

    let response: serde_json::Value = client
        .post(rpc_url)
        .json(&body)
        .send()
        .expect("eth_call request failed — check RPC_URL is reachable")
        .json()
        .expect("eth_call response was not valid JSON");

    if let Some(err) = response.get("error") {
        panic!("eth_call returned an RPC error: {err}");
    }

    let result = response["result"]
        .as_str()
        .expect("eth_call response missing 'result' field");
    from_hex(result)
}

/// Decodes a 32-byte big-endian slot as a u128. Panics if the value
/// doesn't fit (the high 16 bytes must be zero) — real token reserves on
/// these specific pools never approach that range, so a panic here would
/// indicate something has gone genuinely wrong, not an expected case to
/// silently truncate.
fn decode_uint256_as_u128(slot: &[u8]) -> u128 {
    assert_eq!(slot.len(), 32, "expected a 32-byte ABI slot");
    assert!(
        slot[0..16].iter().all(|&b| b == 0),
        "value does not fit in u128 — unexpectedly large on-chain value"
    );
    let mut buf = [0u8; 16];
    buf.copy_from_slice(&slot[16..32]);
    u128::from_be_bytes(buf)
}

fn decode_address(slot: &[u8]) -> String {
    assert_eq!(slot.len(), 32, "expected a 32-byte ABI slot");
    format!("0x{}", to_hex(&slot[12..32]))
}

fn normalize(raw: u128, decimals: u32) -> Decimal {
    Decimal::from(raw) / Decimal::from(10u128.pow(decimals))
}

/// Sends `calls` as a single Multicall3 `aggregate3` batch and returns
/// each call's raw return bytes, in order. See `multicall.rs` for the
/// ABI encode/decode this wraps.
fn multicall_batch(rpc: &str, calls: Vec<Call3>) -> Vec<Vec<u8>> {
    let calldata = multicall::encode_aggregate3(&calls);
    let raw = eth_call(rpc, MULTICALL3_ADDRESS, &calldata);
    multicall::decode_aggregate3_result(&raw)
}

fn run_uniswap_v2(rpc: &str, pool: &str) {
    println!("--- Uniswap v2 pool ({pool}) ---");

    // Round 1 (batched via Multicall3, was 3 separate eth_call round-trips):
    // getReserves() + token0() + token1(). decimals() genuinely can't join
    // this batch — which token is token0 vs token1 isn't knowable until
    // this call returns, so decimals() has a real data dependency on it,
    // not just a stylistic one. See ROADMAP.md's "Multicall batching"
    // entry and multicall.rs's module doc for the honest round-trip count
    // this reduces to (3 -> 1, then a further 2 -> 1 below), not the
    // idealized "N calls -> 1 call" the roadmap entry describes.
    let round1 = vec![
        Call3::new(pool, SEL_GET_RESERVES),
        Call3::new(pool, SEL_TOKEN0),
        Call3::new(pool, SEL_TOKEN1),
    ];
    let round1_results = multicall_batch(rpc, round1);
    let reserves_raw = &round1_results[0];
    // getReserves() returns (uint112 reserve0, uint112 reserve1, uint32
    // blockTimestampLast) — each ABI-encoded in its own 32-byte slot
    // regardless of the underlying Solidity width.
    let reserve0 = decode_uint256_as_u128(&reserves_raw[0..32]);
    let reserve1 = decode_uint256_as_u128(&reserves_raw[32..64]);
    let token0 = decode_address(&round1_results[1]);
    let token1 = decode_address(&round1_results[2]);

    // Round 2 (batched, was 2 separate eth_call round-trips): decimals()
    // on both tokens, now that their addresses are known.
    let round2 = vec![
        Call3::new(&token0, SEL_DECIMALS),
        Call3::new(&token1, SEL_DECIMALS),
    ];
    let round2_results = multicall_batch(rpc, round2);
    let decimals0 = decode_uint256_as_u128(&round2_results[0]) as u32;
    let decimals1 = decode_uint256_as_u128(&round2_results[1]) as u32;

    let x = normalize(reserve0, decimals0);
    let y = normalize(reserve1, decimals1);

    // Uniswap v2's swap fee is a fixed protocol-level constant (0.3%),
    // not exposed via a view function on the pair contract — this is
    // well-established public protocol knowledge, not an invented value,
    // same treatment as citing WooFi's published formula elsewhere in
    // this codebase.
    let fee_retention = dec!(0.997);

    println!("token0={token0} (decimals={decimals0}) reserve={x}");
    println!("token1={token1} (decimals={decimals1}) reserve={y}");

    let pool = Cpmm::new(x, y, fee_retention).expect("failed to construct Cpmm from live reserves");

    // DECISION MADE (this project's stated decision-logging discipline, logged Session 7): this integration has
    // no second, independent price oracle wired in — ARCHITECTURE.md scopes
    // sizing-integration to reading one pool's own state and running
    // sizing-core against it, not integrating an external price feed. Using
    // the pool's own exact marginal price as reference_price would
    // deterministically return NoArbitrageOpportunity (a real, correct
    // result, but not "a real SizingResult" as
    // acceptance criterion requires printing). A documented 0.1% synthetic
    // offset is applied purely to produce a demonstrable Ok(SizingResult)
    // — needs adding to ARCHITECTURE.md § Reference integration.
    let marginal_price = fee_retention * y / x;
    // Escape hatch added per external review: if the caller supplies a
    // real external price via UNISWAP_REFERENCE_PRICE, use it directly —
    // a genuine signal rather than the canned synthetic demo offset.
    let reference_price = match env::var("UNISWAP_REFERENCE_PRICE") {
        Ok(s) => {
            let p = s
                .parse::<Decimal>()
                .expect("UNISWAP_REFERENCE_PRICE must be a valid decimal number");
            println!("pool marginal price = {marginal_price}, using UNISWAP_REFERENCE_PRICE={p}");
            p
        }
        Err(_) => {
            let p = marginal_price * dec!(0.999);
            println!("pool marginal price = {marginal_price}, demo reference_price (0.1% synthetic offset, set UNISWAP_REFERENCE_PRICE for a real signal) = {p}");
            p
        }
    };

    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };
    match pool.optimal_size(reference_price, constraints) {
        Ok(result) => println!(
            "optimal_delta={} expected_profit={} guarantee_tier={:?} iterations={:?}",
            result.optimal_delta, result.expected_profit, result.guarantee_tier, result.iterations
        ),
        Err(e) => println!("optimal_size errored: {e:?}"),
    }
}

fn run_curve_3pool(rpc: &str, pool: &str) {
    println!("--- Curve pool ({pool}) ---");

    // All 5 reads below (balances x3, A, fee) have no dependency on each
    // other, unlike Uniswap's decimals() above — so this is the one
    // genuine "N calls -> 1 round-trip" case, was 5 separate eth_call
    // round-trips.
    let batch = vec![
        Call3::new(pool, &format!("{SEL_BALANCES}{}", encode_uint256_arg(0))),
        Call3::new(pool, &format!("{SEL_BALANCES}{}", encode_uint256_arg(1))),
        Call3::new(pool, &format!("{SEL_BALANCES}{}", encode_uint256_arg(2))),
        Call3::new(pool, SEL_A),
        Call3::new(pool, SEL_FEE),
    ];
    let results = multicall_batch(rpc, batch);
    let bal0_raw = &results[0];
    let bal1_raw = &results[1];
    let bal2_raw = &results[2];

    // DAI/USDC/USDT decimals (18/6/6) are the well-known, immutable
    // decimals of these specific, named token contracts — same treatment
    // as Uniswap v2's fixed 0.3% fee above: public protocol/token
    // knowledge, not an invented value. ARCHITECTURE.md names this pool's
    // composition explicitly as "3pool (DAI/USDC/USDT)".
    let dai = normalize(decode_uint256_as_u128(bal0_raw), 18);
    let usdc = normalize(decode_uint256_as_u128(bal1_raw), 6);
    let usdt = normalize(decode_uint256_as_u128(bal2_raw), 6);

    println!("DAI balance={dai}, USDC balance={usdc}, USDT balance={usdt}");

    let a_raw_value = decode_uint256_as_u128(&results[3]);
    // DECISION MADE (R10, logged Session 7): 3pool is one of Curve's
    // original pool implementations (Vyper 0.2.8, predates the
    // A_PRECISION=100 scaling introduced in later pool versions per
    // Curve's docs), so A() is treated here as the direct, unscaled
    // amplification value by default. NOT verified against live state in
    // this sandbox (no RPC access) — this remains the single highest-risk
    // assumption in this integration.
    //
    // Diagnostic added per external review: print both interpretations
    // side by side so a human can sanity-check which is right before
    // trusting the result, rather than silently committing to one. Real
    // Curve pools' amplification coefficients are, in practice, small
    // integers up to the low thousands (3pool's has historically sat
    // somewhere in the hundreds-to-low-thousands range) — if the unscaled
    // value already looks like a plausible A on that scale, it's very
    // likely correct as-is; if only the /100 value looks plausible and the
    // unscaled one looks implausibly large, A_PRECISION scaling applies
    // and CURVE_A_OVERRIDE (below) should be used.
    let amplification_unscaled = Decimal::from(a_raw_value);
    let amplification_scaled = amplification_unscaled / dec!(100);
    println!(
        "A() raw={a_raw_value} -> unscaled interpretation={amplification_unscaled}, /A_PRECISION(100) interpretation={amplification_scaled}"
    );
    println!(
        "    Using the unscaled interpretation by default. If that looks implausible, re-run with CURVE_A_OVERRIDE=<value> set to the correct one."
    );

    // Escape hatch: let a human who has actually checked Etherscan (or
    // just wants to test with a known value) override the A()
    // interpretation directly, rather than trusting the unverified
    // default above.
    let amplification = env::var("CURVE_A_OVERRIDE")
        .ok()
        .map(|s| {
            s.parse::<Decimal>()
                .expect("CURVE_A_OVERRIDE must be a valid decimal number")
        })
        .unwrap_or(amplification_unscaled);

    // fee() is an integer with 1e10 precision per Curve's docs
    // (<https://curve.readthedocs.io/exchange-pools.html>).
    let fee_fraction = Decimal::from(decode_uint256_as_u128(&results[4])) / dec!(10_000_000_000);
    let fee_retention = Decimal::ONE - fee_fraction;

    println!("A={amplification}, fee_retention={fee_retention}");

    let pool = StableSwap::new(vec![dai, usdc, usdt], amplification, fee_retention)
        .expect("failed to construct StableSwap from live reserves");

    // Same reasoning as the Uniswap v2 case above: approximate the pool's
    // own marginal price via a tiny quote() call (no closed form exists
    // for StableSwap's marginal price), then apply the same documented
    // 0.1% synthetic offset to get a demonstrable Ok(SizingResult).
    let epsilon = dai.max(usdc).max(usdt) * dec!(0.000001);
    let marginal_price = pool
        .quote(epsilon)
        .expect("failed to quote a small trade for marginal price estimation")
        / epsilon;
    let reference_price = match env::var("CURVE_REFERENCE_PRICE") {
        Ok(s) => {
            let p = s
                .parse::<Decimal>()
                .expect("CURVE_REFERENCE_PRICE must be a valid decimal number");
            println!(
                "pool marginal price (est.) = {marginal_price}, using CURVE_REFERENCE_PRICE={p}"
            );
            p
        }
        Err(_) => {
            let p = marginal_price * dec!(0.999);
            println!("pool marginal price (est.) = {marginal_price}, demo reference_price (0.1% synthetic offset, set CURVE_REFERENCE_PRICE for a real signal) = {p}");
            p
        }
    };

    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };
    match pool.optimal_size(reference_price, constraints) {
        Ok(result) => println!(
            "optimal_delta={} expected_profit={} guarantee_tier={:?} iterations={:?}",
            result.optimal_delta, result.expected_profit, result.guarantee_tier, result.iterations
        ),
        Err(e) => println!("optimal_size errored: {e:?}"),
    }
}

/// Multi-chain support (added — `ROADMAP.md` "Multi-chain support in
/// `sizing-integration`"): `RPC_URL` already pointed at any EVM chain's
/// RPC endpoint, since it's just a URL — the missing piece the roadmap
/// entry actually calls out was the two pool addresses being hardcoded
/// to mainnet-specific deployments. `UNISWAP_POOL`/`CURVE_POOL` env vars
/// let a caller point this at the equivalent pools on Arbitrum, Base,
/// Optimism, or any other EVM chain with the same pool contracts
/// deployed, defaulting to the original mainnet addresses if unset — an
/// existing-behavior-preserving default, not a silently changed one.
/// `MULTICALL3_ADDRESS` in `multicall.rs` needs no equivalent change:
/// it's the same deterministic address on essentially every EVM chain
/// already (see that module's doc comment).
///
/// **What this does NOT handle** (per the roadmap entry's own
/// parenthetical "handling L2-specific quirks, if any" — logged here
/// per this project's stated decision-logging discipline rather than silently assumed away): L2-specific
/// `eth_call` gas/response quirks, if any exist for a given L2's RPC
/// implementation, are untested — this sandbox has no reachable RPC for
/// any chain, mainnet or L2, so none of this has been run live.
fn pool_addresses() -> (String, String) {
    let uniswap_pool =
        env::var("UNISWAP_POOL").unwrap_or_else(|_| UNISWAP_V2_USDC_WETH_DEFAULT.to_string());
    let curve_pool = env::var("CURVE_POOL").unwrap_or_else(|_| CURVE_3POOL_DEFAULT.to_string());
    (uniswap_pool, curve_pool)
}

fn main() {
    let rpc = rpc_url();
    let (uniswap_pool, curve_pool) = pool_addresses();
    run_uniswap_v2(&rpc, &uniswap_pool);
    println!();
    run_curve_3pool(&rpc, &curve_pool);
}
