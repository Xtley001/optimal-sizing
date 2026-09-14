//! Property-based tests for the shared `NetProfit` fixed-cost/capacity
//! layer, per `docs/TESTING.md § net_profit_proptest.rs`. Each named
//! property covers all three curve families within a single test function,
//! per TESTING.md's "For all three curves..." phrasing.

use proptest::prelude::*;
use rust_decimal::Decimal;
use sizing_core::curves::{Cpmm, Pmm, StableSwap};
use sizing_core::{SizingAlgorithm, SizingConstraints, SizingError};

fn reserve_strategy() -> impl Strategy<Value = Decimal> {
    (1_000.0f64..1e9).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

fn discount_strategy() -> impl Strategy<Value = f64> {
    0.5f64..0.999f64
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_domain_restriction_excludes_unprofitable_sizes(
        x in reserve_strategy(),
        y in reserve_strategy(),
        discount in discount_strategy(),
        fixed_cost_fraction in 0.01f64..0.9f64,
        ss_reserve_a in reserve_strategy(),
        ss_reserve_b in reserve_strategy(),
        ss_discount in discount_strategy(),
        ss_fixed_cost_fraction in 0.01f64..0.9f64,
        pmm_base in reserve_strategy(),
        pmm_discount in discount_strategy(),
        pmm_fixed_cost_fraction in 0.01f64..0.9f64,
    ) {
        // --- Cpmm ---
        let fee = Decimal::new(997, 3);
        let pool = Cpmm::new(x, y, fee).unwrap();
        let marginal = fee * y / x;
        let reference_price = marginal * Decimal::from_f64_retain(discount).unwrap();
        if reference_price > Decimal::ZERO {
            let zero_cost = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
            if let Ok(unconstrained) = pool.optimal_size(reference_price, zero_cost) {
                let gross_max = unconstrained.expected_profit;
                if gross_max > Decimal::ZERO {
                    let small_cost = gross_max * Decimal::from_f64_retain(fixed_cost_fraction).unwrap();
                    let constraints = SizingConstraints { fixed_cost: small_cost, max_size: None };
                    if let Ok(result) = pool.optimal_size(reference_price, constraints) {
                        prop_assert!(result.expected_profit > Decimal::ZERO);
                    }

                    let large_cost = gross_max + Decimal::ONE;
                    let constraints2 = SizingConstraints { fixed_cost: large_cost, max_size: None };
                    let result2 = pool.optimal_size(reference_price, constraints2);
                    prop_assert_eq!(result2, Err(SizingError::NoProfitableSize));
                }
            }
        }

        // --- StableSwap ---
        let reserves = vec![ss_reserve_a, ss_reserve_b];
        if let Ok(ss_pool) = StableSwap::new(reserves, Decimal::from(100), Decimal::new(997, 3)) {
            let reference_price = ss_reserve_a.min(ss_reserve_b) / ss_reserve_a.max(ss_reserve_b)
                * Decimal::from_f64_retain(ss_discount).unwrap();
            if reference_price > Decimal::ZERO {
                let zero_cost = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
                if let Ok(unconstrained) = ss_pool.optimal_size(reference_price, zero_cost) {
                    let gross_max = unconstrained.expected_profit;
                    if gross_max > Decimal::ZERO {
                        let small_cost = gross_max * Decimal::from_f64_retain(ss_fixed_cost_fraction).unwrap();
                        let constraints = SizingConstraints { fixed_cost: small_cost, max_size: None };
                        if let Ok(result) = ss_pool.optimal_size(reference_price, constraints) {
                            prop_assert!(result.expected_profit > Decimal::ZERO);
                        }

                        let large_cost = gross_max + Decimal::ONE;
                        let constraints2 = SizingConstraints { fixed_cost: large_cost, max_size: None };
                        let result2 = ss_pool.optimal_size(reference_price, constraints2);
                        prop_assert_eq!(result2, Err(SizingError::NoProfitableSize));
                    }
                }
            }
        }

        // --- Pmm ---
        let oracle_price = Decimal::from(2000);
        let k = Decimal::new(1, 7); // 0.0000001
        let quote_reserve = pmm_base * oracle_price;
        if let Ok(pmm_pool) = Pmm::new(pmm_base, quote_reserve, oracle_price, k) {
            let reference_price = oracle_price * Decimal::from_f64_retain(pmm_discount).unwrap();
            let zero_cost = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
            if let Ok(unconstrained) = pmm_pool.optimal_size(reference_price, zero_cost) {
                let gross_max = unconstrained.expected_profit;
                if gross_max > Decimal::ZERO {
                    let small_cost = gross_max * Decimal::from_f64_retain(pmm_fixed_cost_fraction).unwrap();
                    let constraints = SizingConstraints { fixed_cost: small_cost, max_size: None };
                    if let Ok(result) = pmm_pool.optimal_size(reference_price, constraints) {
                        prop_assert!(result.expected_profit > Decimal::ZERO);
                    }

                    let large_cost = gross_max + Decimal::ONE;
                    let constraints2 = SizingConstraints { fixed_cost: large_cost, max_size: None };
                    let result2 = pmm_pool.optimal_size(reference_price, constraints2);
                    prop_assert_eq!(result2, Err(SizingError::NoProfitableSize));
                }
            }
        }
    }

    #[test]
    fn prop_max_size_respected(
        x in reserve_strategy(),
        y in reserve_strategy(),
        discount in discount_strategy(),
        max_size_fraction in 0.01f64..0.9f64,
        ss_reserve_a in reserve_strategy(),
        ss_reserve_b in reserve_strategy(),
        ss_discount in discount_strategy(),
        ss_max_size_fraction in 0.01f64..0.9f64,
        pmm_base in reserve_strategy(),
        pmm_discount in discount_strategy(),
        pmm_max_size_fraction in 0.01f64..0.9f64,
    ) {
        // --- Cpmm ---
        let fee = Decimal::new(997, 3);
        let pool = Cpmm::new(x, y, fee).unwrap();
        let marginal = fee * y / x;
        let reference_price = marginal * Decimal::from_f64_retain(discount).unwrap();
        if reference_price > Decimal::ZERO {
            let zero_cost = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
            if let Ok(unconstrained) = pool.optimal_size(reference_price, zero_cost) {
                if unconstrained.optimal_delta > Decimal::ZERO {
                    let max_size = unconstrained.optimal_delta * Decimal::from_f64_retain(max_size_fraction).unwrap();
                    let constraints = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: Some(max_size) };
                    if let Ok(result) = pool.optimal_size(reference_price, constraints) {
                        prop_assert!(result.optimal_delta <= max_size);
                    }
                }
            }
        }

        // --- StableSwap ---
        let reserves = vec![ss_reserve_a, ss_reserve_b];
        if let Ok(ss_pool) = StableSwap::new(reserves, Decimal::from(100), Decimal::new(997, 3)) {
            let reference_price = ss_reserve_a.min(ss_reserve_b) / ss_reserve_a.max(ss_reserve_b)
                * Decimal::from_f64_retain(ss_discount).unwrap();
            if reference_price > Decimal::ZERO {
                let zero_cost = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
                if let Ok(unconstrained) = ss_pool.optimal_size(reference_price, zero_cost) {
                    if unconstrained.optimal_delta > Decimal::ZERO {
                        let max_size = unconstrained.optimal_delta * Decimal::from_f64_retain(ss_max_size_fraction).unwrap();
                        let constraints = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: Some(max_size) };
                        if let Ok(result) = ss_pool.optimal_size(reference_price, constraints) {
                            prop_assert!(result.optimal_delta <= max_size);
                        }
                    }
                }
            }
        }

        // --- Pmm ---
        let oracle_price = Decimal::from(2000);
        let k = Decimal::new(1, 7);
        let quote_reserve = pmm_base * oracle_price;
        if let Ok(pmm_pool) = Pmm::new(pmm_base, quote_reserve, oracle_price, k) {
            let reference_price = oracle_price * Decimal::from_f64_retain(pmm_discount).unwrap();
            let zero_cost = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
            if let Ok(unconstrained) = pmm_pool.optimal_size(reference_price, zero_cost) {
                if unconstrained.optimal_delta > Decimal::ZERO {
                    let max_size = unconstrained.optimal_delta * Decimal::from_f64_retain(pmm_max_size_fraction).unwrap();
                    let constraints = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: Some(max_size) };
                    if let Ok(result) = pmm_pool.optimal_size(reference_price, constraints) {
                        prop_assert!(result.optimal_delta <= max_size);
                    }
                }
            }
        }
    }
}
