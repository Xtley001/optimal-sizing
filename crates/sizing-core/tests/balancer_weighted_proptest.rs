//! Property-based tests for Balancer weighted pools ($V = \prod B_i^{w_i}$).
//! Per `docs/TESTING.md` and `docs/CURVES_SPEC.md § 2`.

use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::{BalancerWeightedPool, Cpmm};
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_balancer_quote_monotonic(
        bi in 100_000u64..5_000_000u64,
        bo in 100_000u64..5_000_000u64,
        w_raw in 20u32..80u32,
        delta1 in 10u64..1_000u64,
        delta2 in 1_001u64..10_000u64,
    ) {
        let wi = Decimal::from(w_raw) / dec!(100);
        let wo = Decimal::ONE - wi;

        let pool = BalancerWeightedPool::new(
            vec![Decimal::from(bi), Decimal::from(bo)],
            vec![wi, wo],
            dec!(0.997),
            0,
            1,
        ).unwrap();

        let d1 = Decimal::from(delta1);
        let d2 = Decimal::from(delta2);

        let out1 = pool.quote(d1).unwrap();
        let out2 = pool.quote(d2).unwrap();

        prop_assert!(out2 >= out1, "quote must be monotonically increasing: out2 ({}) >= out1 ({})", out2, out1);
    }

    #[test]
    fn prop_balancer_cpmm_equivalence_at_equal_weights(
        bi in 500_000u64..5_000_000u64,
        bo in 500_000u64..5_000_000u64,
    ) {
        let r_in = Decimal::from(bi);
        let r_out = Decimal::from(bo);
        let f = dec!(0.997);

        let balancer = BalancerWeightedPool::new(
            vec![r_in, r_out],
            vec![dec!(0.5), dec!(0.5)],
            f,
            0,
            1,
        ).unwrap();

        let cpmm = Cpmm::new(r_in, r_out, f).unwrap();

        let ref_price = (r_out * f / r_in) * dec!(0.8); // guarantee arbitrage

        if let (Ok(b_res), Ok(c_res)) = (
            balancer.optimal_size(ref_price, SizingConstraints { fixed_cost: dec!(0), max_size: None }),
            cpmm.optimal_size(ref_price, SizingConstraints { fixed_cost: dec!(0), max_size: None }),
        ) {
            prop_assert_eq!(b_res.guarantee_tier, GuaranteeTier::ProvenOptimal);
            let delta_diff = (b_res.optimal_delta - c_res.optimal_delta).abs();
            let tol = c_res.optimal_delta * dec!(0.0001);
            prop_assert!(delta_diff <= tol, "balancer delta ({}) should match cpmm delta ({}) within tol ({})", b_res.optimal_delta, c_res.optimal_delta, tol);
        }
    }

    #[test]
    fn prop_balancer_optimality_beats_perturbations(
        bi in 500_000u64..5_000_000u64,
        bo in 500_000u64..5_000_000u64,
        w_raw in 20u32..80u32,
    ) {
        let wi = Decimal::from(w_raw) / dec!(100);
        let wo = Decimal::ONE - wi;
        let r_in = Decimal::from(bi);
        let r_out = Decimal::from(bo);
        let f = dec!(0.997);

        let pool = BalancerWeightedPool::new(
            vec![r_in, r_out],
            vec![wi, wo],
            f,
            0,
            1,
        ).unwrap();

        let marginal_p = (wi / wo) * (r_out * f / r_in);
        let ref_price = marginal_p * dec!(0.75);

        if let Ok(res) = pool.optimal_size(ref_price, SizingConstraints { fixed_cost: dec!(0), max_size: None }) {
            prop_assert_eq!(res.guarantee_tier, GuaranteeTier::ProvenOptimal);
            let opt_delta = res.optimal_delta;
            let opt_profit = res.expected_profit;

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
