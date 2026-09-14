//! Property-based tests for multi-tick concentrated liquidity (CLMM).
//! Per `docs/TESTING.md` and `docs/CURVES_SPEC.md § 1`.

use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::{ConcentratedLiquidity, TickRange};
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_multi_tick_quote_monotonic(
        l1 in 100_000u64..1_000_000u64,
        l2 in 100_000u64..1_000_000u64,
        delta1 in 100u64..5_000u64,
        delta2 in 5_001u64..20_000u64,
    ) {
        let tick1 = TickRange {
            lower_sqrt_price: dec!(0.90),
            upper_sqrt_price: dec!(1.00),
            liquidity_gross: Decimal::from(l1),
            liquidity_net: dec!(0),
        };
        let tick2 = TickRange {
            lower_sqrt_price: dec!(1.00),
            upper_sqrt_price: dec!(1.10),
            liquidity_gross: Decimal::from(l2),
            liquidity_net: dec!(0),
        };

        let pool = ConcentratedLiquidity::new_multi_tick(vec![tick1, tick2], dec!(1.05), dec!(0.997)).unwrap();

        let d1 = Decimal::from(delta1);
        let d2 = Decimal::from(delta2);

        let out1 = pool.quote(d1).unwrap();
        let out2 = pool.quote(d2).unwrap();

        prop_assert!(out2 >= out1, "out2 ({}) should be >= out1 ({})", out2, out1);
    }

    #[test]
    fn prop_multi_tick_optimality_beats_perturbations(
        l1 in 500_000u64..2_000_000u64,
        l2 in 500_000u64..2_000_000u64,
    ) {
        let tick1 = TickRange {
            lower_sqrt_price: dec!(0.80),
            upper_sqrt_price: dec!(1.00),
            liquidity_gross: Decimal::from(l1),
            liquidity_net: dec!(0),
        };
        let tick2 = TickRange {
            lower_sqrt_price: dec!(1.00),
            upper_sqrt_price: dec!(1.20),
            liquidity_gross: Decimal::from(l2),
            liquidity_net: dec!(0),
        };

        let pool = ConcentratedLiquidity::new_multi_tick(vec![tick1, tick2], dec!(1.10), dec!(0.997)).unwrap();
        let ref_price = dec!(0.85);

        if let Ok(res) = pool.optimal_size(ref_price, SizingConstraints { fixed_cost: dec!(0), max_size: None }) {
            prop_assert_eq!(res.guarantee_tier, GuaranteeTier::ProvenOptimal);
            let opt_delta = res.optimal_delta;
            let opt_profit = res.expected_profit;

            // Perturbation check: 90% and 110%
            let d_down = opt_delta * dec!(0.90);
            let d_up = opt_delta * dec!(1.10);

            if let Ok(out_down) = pool.quote(d_down) {
                let profit_down = out_down - ref_price * d_down;
                prop_assert!(opt_profit >= profit_down, "opt_profit ({}) must be >= profit_down ({})", opt_profit, profit_down);
            }

            if let Ok(out_up) = pool.quote(d_up) {
                let profit_up = out_up - ref_price * d_up;
                prop_assert!(opt_profit >= profit_up, "opt_profit ({}) must be >= profit_up ({})", opt_profit, profit_up);
            }
        }
    }
}
