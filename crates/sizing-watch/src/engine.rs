//! Sizing engine and opportunity emitter for `sizing-watch`.
//! Per `docs/STREAMING_SPEC.md § 4 & § 5`.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use sizing_core::curves::{
    BalancerWeightedPool, ConcentratedLiquidity, Cpmm, CurveCryptoSwap, DodoPmm, Pmm, StableSwap,
};
use sizing_core::traits::SizingAlgorithm;
use sizing_core::types::{GuaranteeTier, SizingConstraints};
use sizing_router::{Leg, Route};

use crate::cache::PoolCache;
use crate::config::{PoolConfig, WatchConfig};

/// Output event format emitted as single-line JSON to stdout when opportunity is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityEvent {
    /// Event name: "OPPORTUNITY_DETECTED".
    pub event: String,
    /// Epoch timestamp in nanoseconds.
    pub timestamp_ns: u128,
    /// Block number when opportunity was evaluated.
    pub block_number: u64,
    /// Identifier of the target pool or route.
    pub target: String,
    /// Profit-maximizing trade size $\Delta x^*$.
    pub optimal_delta: String,
    /// Net expected profit in USD after fixed costs.
    pub expected_profit_usd: String,
    /// Mathematical guarantee tier.
    pub guarantee_tier: String,
    /// Reference price used for evaluation.
    pub reference_price: String,
    /// Fixed execution cost in USD.
    pub gas_cost_usd: String,
    /// Pool state block number.
    pub pool_state_block: u64,
}

fn tier_display(tier: &GuaranteeTier) -> String {
    match tier {
        GuaranteeTier::ProvenOptimal => "ProvenOptimal".to_string(),
        GuaranteeTier::NumericallyGuaranteed {
            convergence_conditions_met,
        } => {
            format!("NumericallyGuaranteed {{ convergence_conditions_met: {convergence_conditions_met} }}")
        }
        GuaranteeTier::EmpiricallyValidated {
            unimodality_confirmed,
        } => {
            format!("EmpiricallyValidated {{ unimodality_confirmed: {unimodality_confirmed} }}")
        }
        GuaranteeTier::ComposedFrom(legs) => {
            let inner: Vec<String> = legs.iter().map(tier_display).collect();
            format!("ComposedFrom([{}])", inner.join(", "))
        }
    }
}

/// Constructs a `Leg` from a pool's static config and current dynamic reserves.
pub fn build_leg(
    config: &PoolConfig,
    reserves: &[Decimal],
    sqrt_p_curr: Option<Decimal>,
) -> Option<Leg> {
    match config.curve_type.to_lowercase().as_str() {
        "cpmm" => {
            if reserves.len() < 2 {
                return None;
            }
            let x = reserves[config.input_token_index];
            let y = reserves[config.output_token_index];
            Cpmm::new(x, y, config.fee_retention).ok().map(Leg::Cpmm)
        }
        "stableswap" => {
            let amp = config.amplification.unwrap_or(Decimal::from(100));
            StableSwap::new(reserves.to_vec(), amp, config.fee_retention)
                .ok()
                .map(Leg::StableSwap)
        }
        "pmm" => {
            if reserves.len() < 2 {
                return None;
            }
            let b = reserves[0];
            let q = reserves[1];
            let oracle = Decimal::ONE; // Default guide price
            let k = Decimal::from_str_exact("0.0001").unwrap();
            Pmm::new(b, q, oracle, k).ok().map(Leg::Pmm)
        }
        "clmm" => {
            let p_curr = sqrt_p_curr.unwrap_or(Decimal::ONE);
            let liquidity = if !reserves.is_empty() {
                reserves[0]
            } else {
                Decimal::from(1_000_000)
            };
            ConcentratedLiquidity::new(
                liquidity,
                p_curr * Decimal::from_str_exact("0.8").unwrap(),
                p_curr * Decimal::from_str_exact("1.2").unwrap(),
                p_curr,
                config.fee_retention,
            )
            .ok()
            .map(Leg::ConcentratedLiquidity)
        }
        "balancer" => {
            let weights = config.weights.clone().unwrap_or_else(|| {
                vec![
                    Decimal::from_str_exact("0.5").unwrap(),
                    Decimal::from_str_exact("0.5").unwrap(),
                ]
            });
            BalancerWeightedPool::new(
                reserves.to_vec(),
                weights,
                config.fee_retention,
                config.input_token_index,
                config.output_token_index,
            )
            .ok()
            .map(Leg::BalancerWeighted)
        }
        "dodo" => {
            if reserves.len() < 2 {
                return None;
            }
            let b = reserves[0];
            let q = reserves[1];
            let k = Decimal::from_str_exact("0.1").unwrap();
            DodoPmm::new(b, q, b, q, Decimal::ONE, k, config.fee_retention, true)
                .ok()
                .map(Leg::DodoPmm)
        }
        "cryptoswap" => {
            if reserves.len() < 2 {
                return None;
            }
            let a = config.amplification.unwrap_or(Decimal::from(400_000));
            let gamma = config
                .gamma
                .unwrap_or(Decimal::from_str_exact("0.0001").unwrap());
            CurveCryptoSwap::new(
                [reserves[0], reserves[1]],
                a,
                gamma,
                config.fee_retention,
                config.input_token_index,
            )
            .ok()
            .map(Leg::CurveCryptoSwap)
        }
        _ => None,
    }
}

/// Evaluates all configured pools and routes against a given reference price and emits opportunities.
pub fn evaluate_opportunities(
    config: &WatchConfig,
    cache: &PoolCache,
    reference_price: Decimal,
    gas_price_gwei: Decimal,
) {
    let block_num = cache.latest_block();
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Gas cost calculation: gas_units * gas_price * 1e-9 * eth_price (~$3000)
    let gas_cost_usd = Decimal::from(config.default_fixed_cost_gas_units)
        * gas_price_gwei
        * Decimal::from_str_exact("0.000000001").unwrap()
        * Decimal::from(3000);

    let constraints = SizingConstraints {
        fixed_cost: gas_cost_usd,
        max_size: None,
    };

    // 1. Evaluate individual pools
    for pool_cfg in &config.pools {
        if let Some(state) = cache.get_pool(&pool_cfg.id) {
            if let Some(leg) = build_leg(pool_cfg, &state.reserves, state.sqrt_price_current) {
                let sizing_res = match &leg {
                    Leg::Cpmm(c) => c.optimal_size(reference_price, constraints.clone()),
                    Leg::StableSwap(s) => s.optimal_size(reference_price, constraints.clone()),
                    Leg::Pmm(p) => p.optimal_size(reference_price, constraints.clone()),
                    Leg::ConcentratedLiquidity(cl) => {
                        cl.optimal_size(reference_price, constraints.clone())
                    }
                    Leg::BalancerWeighted(b) => {
                        b.optimal_size(reference_price, constraints.clone())
                    }
                    Leg::DodoPmm(d) => d.optimal_size(reference_price, constraints.clone()),
                    Leg::CurveCryptoSwap(cs) => {
                        cs.optimal_size(reference_price, constraints.clone())
                    }
                };

                if let Ok(res) = sizing_res {
                    if res.expected_profit >= config.min_profit_usd {
                        let event = OpportunityEvent {
                            event: "OPPORTUNITY_DETECTED".to_string(),
                            timestamp_ns: now_ns,
                            block_number: block_num,
                            target: pool_cfg.id.clone(),
                            optimal_delta: res.optimal_delta.to_string(),
                            expected_profit_usd: res.expected_profit.to_string(),
                            guarantee_tier: tier_display(&res.guarantee_tier),
                            reference_price: reference_price.to_string(),
                            gas_cost_usd: gas_cost_usd.to_string(),
                            pool_state_block: state.last_block_number,
                        };

                        if let Ok(json) = serde_json::to_string(&event) {
                            println!("{json}");
                        }
                    }
                }
            }
        }
    }

    // 2. Evaluate multi-hop routes
    for route_cfg in &config.routes {
        let mut legs = Vec::new();
        let mut min_block = u64::MAX;

        for pool_id in &route_cfg.legs {
            if let Some(pool_cfg) = config.pools.iter().find(|p| p.id == *pool_id) {
                if let Some(state) = cache.get_pool(pool_id) {
                    min_block = min_block.min(state.last_block_number);
                    if let Some(leg) =
                        build_leg(pool_cfg, &state.reserves, state.sqrt_price_current)
                    {
                        legs.push(leg);
                    }
                }
            }
        }

        if legs.len() == route_cfg.legs.len() && legs.len() >= 2 {
            if let Ok(route) = Route::new_multi_hop(legs) {
                if let Ok(res) = route.optimal_size(reference_price, constraints.clone()) {
                    if res.expected_profit >= config.min_profit_usd {
                        let event = OpportunityEvent {
                            event: "OPPORTUNITY_DETECTED".to_string(),
                            timestamp_ns: now_ns,
                            block_number: block_num,
                            target: route_cfg.id.clone(),
                            optimal_delta: res.optimal_delta.to_string(),
                            expected_profit_usd: res.expected_profit.to_string(),
                            guarantee_tier: tier_display(&res.guarantee_tier),
                            reference_price: reference_price.to_string(),
                            gas_cost_usd: gas_cost_usd.to_string(),
                            pool_state_block: if min_block == u64::MAX {
                                block_num
                            } else {
                                min_block
                            },
                        };

                        if let Ok(json) = serde_json::to_string(&event) {
                            println!("{json}");
                        }
                    }
                }
            }
        }
    }
}
