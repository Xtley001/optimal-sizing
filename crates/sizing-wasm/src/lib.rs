//! `sizing-wasm` — thin `wasm-bindgen` wrapper around `sizing-core`'s curve families.
//! Boundary-only: converts JS-friendly decimal strings to/from `Decimal` at the FFI edge.

use rust_decimal::Decimal;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

use sizing_core::curves::{BalancerWeightedPool, ConcentratedLiquidity, Cpmm, Pmm, StableSwap};
use sizing_core::traits::SizingAlgorithm;
use sizing_core::types::SizingConstraints;

fn parse_decimal(s: &str, field: &str) -> Result<Decimal, JsValue> {
    Decimal::from_str(s).map_err(|e| JsValue::from_str(&format!("invalid {field}: {e}")))
}

/// Result of a sizing call, JS-friendly.
#[wasm_bindgen]
pub struct WasmSizingResult {
    ok: bool,
    optimal_delta: String,
    expected_profit: String,
    guarantee_tier: String,
    iterations: Option<u32>,
    error: String,
}

#[wasm_bindgen]
impl WasmSizingResult {
    /// `true` if a profitable size was found.
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }
    /// The profit-maximizing trade size, as a decimal string. Empty if `ok` is `false`.
    #[wasm_bindgen(getter)]
    pub fn optimal_delta(&self) -> String {
        self.optimal_delta.clone()
    }
    /// Net expected profit at `optimal_delta`, as a decimal string. Empty if `ok` is `false`.
    #[wasm_bindgen(getter)]
    pub fn expected_profit(&self) -> String {
        self.expected_profit.clone()
    }
    /// Short tag naming the `GuaranteeTier` variant.
    #[wasm_bindgen(getter)]
    pub fn guarantee_tier(&self) -> String {
        self.guarantee_tier.clone()
    }
    /// Iteration count for iterative methods, `-1` if not applicable.
    #[wasm_bindgen(getter)]
    pub fn iterations(&self) -> i64 {
        self.iterations.map(|n| n as i64).unwrap_or(-1)
    }
    /// Human-readable error detail. Empty if `ok` is `true`.
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> String {
        self.error.clone()
    }
}

fn tier_tag(tier: &sizing_core::types::GuaranteeTier) -> String {
    use sizing_core::types::GuaranteeTier::*;
    match tier {
        ProvenOptimal => "ProvenOptimal".to_string(),
        NumericallyGuaranteed { .. } => "NumericallyGuaranteed".to_string(),
        EmpiricallyValidated { .. } => "EmpiricallyValidated".to_string(),
        ComposedFrom(_) => "ComposedFrom".to_string(),
    }
}

fn err_result(detail: String) -> WasmSizingResult {
    WasmSizingResult {
        ok: false,
        optimal_delta: String::new(),
        expected_profit: String::new(),
        guarantee_tier: String::new(),
        iterations: None,
        error: detail,
    }
}

/// Sizes a constant-product (`x*y=k`) pool.
#[wasm_bindgen]
pub fn cpmm_optimal_size(
    x: &str,
    y: &str,
    fee_retention: &str,
    reference_price: &str,
    fixed_cost: &str,
    max_size: Option<String>,
) -> WasmSizingResult {
    let run = || -> Result<WasmSizingResult, JsValue> {
        let x = parse_decimal(x, "x")?;
        let y = parse_decimal(y, "y")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
        let max_size = max_size
            .map(|s| parse_decimal(&s, "max_size"))
            .transpose()?;

        let pool =
            Cpmm::new(x, y, fee_retention).map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        match pool.optimal_size(
            reference_price,
            SizingConstraints {
                fixed_cost,
                max_size,
            },
        ) {
            Ok(r) => Ok(WasmSizingResult {
                ok: true,
                optimal_delta: r.optimal_delta.to_string(),
                expected_profit: r.expected_profit.to_string(),
                guarantee_tier: tier_tag(&r.guarantee_tier),
                iterations: r.iterations,
                error: String::new(),
            }),
            Err(e) => Ok(err_result(format!("{e:?}"))),
        }
    };
    run().unwrap_or_else(|e: JsValue| {
        err_result(e.as_string().unwrap_or_else(|| "invalid input".to_string()))
    })
}

/// Sizes a StableSwap-invariant pool.
#[wasm_bindgen]
pub fn stableswap_optimal_size(
    reserves: Vec<String>,
    amplification: &str,
    fee_retention: &str,
    reference_price: &str,
    fixed_cost: &str,
    max_size: Option<String>,
) -> WasmSizingResult {
    let run = || -> Result<WasmSizingResult, JsValue> {
        let reserves: Result<Vec<Decimal>, JsValue> = reserves
            .iter()
            .map(|s| parse_decimal(s, "reserves[i]"))
            .collect();
        let reserves = reserves?;
        let amplification = parse_decimal(amplification, "amplification")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
        let max_size = max_size
            .map(|s| parse_decimal(&s, "max_size"))
            .transpose()?;

        let pool = StableSwap::new(reserves, amplification, fee_retention)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        match pool.optimal_size(
            reference_price,
            SizingConstraints {
                fixed_cost,
                max_size,
            },
        ) {
            Ok(r) => Ok(WasmSizingResult {
                ok: true,
                optimal_delta: r.optimal_delta.to_string(),
                expected_profit: r.expected_profit.to_string(),
                guarantee_tier: tier_tag(&r.guarantee_tier),
                iterations: r.iterations,
                error: String::new(),
            }),
            Err(e) => Ok(err_result(format!("{e:?}"))),
        }
    };
    run().unwrap_or_else(|e: JsValue| {
        err_result(e.as_string().unwrap_or_else(|| "invalid input".to_string()))
    })
}

/// Sizes a WooFi-style PMM pool.
#[wasm_bindgen]
pub fn pmm_optimal_size(
    base_reserve: &str,
    quote_reserve: &str,
    oracle_price: &str,
    k: &str,
    reference_price: &str,
    fixed_cost: &str,
    max_size: Option<String>,
) -> WasmSizingResult {
    let run = || -> Result<WasmSizingResult, JsValue> {
        let base_reserve = parse_decimal(base_reserve, "base_reserve")?;
        let quote_reserve = parse_decimal(quote_reserve, "quote_reserve")?;
        let oracle_price = parse_decimal(oracle_price, "oracle_price")?;
        let k = parse_decimal(k, "k")?;
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
        let max_size = max_size
            .map(|s| parse_decimal(&s, "max_size"))
            .transpose()?;

        let pool = Pmm::new(base_reserve, quote_reserve, oracle_price, k)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        match pool.optimal_size(
            reference_price,
            SizingConstraints {
                fixed_cost,
                max_size,
            },
        ) {
            Ok(r) => Ok(WasmSizingResult {
                ok: true,
                optimal_delta: r.optimal_delta.to_string(),
                expected_profit: r.expected_profit.to_string(),
                guarantee_tier: tier_tag(&r.guarantee_tier),
                iterations: r.iterations,
                error: String::new(),
            }),
            Err(e) => Ok(err_result(format!("{e:?}"))),
        }
    };
    run().unwrap_or_else(|e: JsValue| {
        err_result(e.as_string().unwrap_or_else(|| "invalid input".to_string()))
    })
}

/// Sizes a concentrated-liquidity position.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn clmm_optimal_size(
    liquidity: &str,
    sqrt_price_lower: &str,
    sqrt_price_upper: &str,
    sqrt_price_current: &str,
    fee_retention: &str,
    reference_price: &str,
    fixed_cost: &str,
    max_size: Option<String>,
) -> WasmSizingResult {
    let run = || -> Result<WasmSizingResult, JsValue> {
        let liquidity = parse_decimal(liquidity, "liquidity")?;
        let sqrt_price_lower = parse_decimal(sqrt_price_lower, "sqrt_price_lower")?;
        let sqrt_price_upper = parse_decimal(sqrt_price_upper, "sqrt_price_upper")?;
        let sqrt_price_current = parse_decimal(sqrt_price_current, "sqrt_price_current")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
        let max_size = max_size
            .map(|s| parse_decimal(&s, "max_size"))
            .transpose()?;

        let pool = ConcentratedLiquidity::new(
            liquidity,
            sqrt_price_lower,
            sqrt_price_upper,
            sqrt_price_current,
            fee_retention,
        )
        .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        match pool.optimal_size(
            reference_price,
            SizingConstraints {
                fixed_cost,
                max_size,
            },
        ) {
            Ok(r) => Ok(WasmSizingResult {
                ok: true,
                optimal_delta: r.optimal_delta.to_string(),
                expected_profit: r.expected_profit.to_string(),
                guarantee_tier: tier_tag(&r.guarantee_tier),
                iterations: r.iterations,
                error: String::new(),
            }),
            Err(e) => Ok(err_result(format!("{e:?}"))),
        }
    };
    run().unwrap_or_else(|e: JsValue| {
        err_result(e.as_string().unwrap_or_else(|| "invalid input".to_string()))
    })
}

/// Sizes a Balancer weighted pool.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn balancer_optimal_size(
    reserves: Vec<String>,
    weights: Vec<String>,
    fee_retention: &str,
    input_index: usize,
    output_index: usize,
    reference_price: &str,
    fixed_cost: &str,
    max_size: Option<String>,
) -> WasmSizingResult {
    let run = || -> Result<WasmSizingResult, JsValue> {
        let reserves: Result<Vec<Decimal>, JsValue> = reserves
            .iter()
            .map(|s| parse_decimal(s, "reserves[i]"))
            .collect();
        let reserves = reserves?;
        let weights: Result<Vec<Decimal>, JsValue> = weights
            .iter()
            .map(|s| parse_decimal(s, "weights[i]"))
            .collect();
        let weights = weights?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
        let max_size = max_size
            .map(|s| parse_decimal(&s, "max_size"))
            .transpose()?;

        let pool =
            BalancerWeightedPool::new(reserves, weights, fee_retention, input_index, output_index)
                .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        match pool.optimal_size(
            reference_price,
            SizingConstraints {
                fixed_cost,
                max_size,
            },
        ) {
            Ok(r) => Ok(WasmSizingResult {
                ok: true,
                optimal_delta: r.optimal_delta.to_string(),
                expected_profit: r.expected_profit.to_string(),
                guarantee_tier: tier_tag(&r.guarantee_tier),
                iterations: r.iterations,
                error: String::new(),
            }),
            Err(e) => Ok(err_result(format!("{e:?}"))),
        }
    };
    run().unwrap_or_else(|e: JsValue| {
        err_result(e.as_string().unwrap_or_else(|| "invalid input".to_string()))
    })
}
