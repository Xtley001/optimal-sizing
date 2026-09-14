//! Property-based tests for DODO Proactive Market Maker (PMM).
//! Per `docs/TESTING.md` and `docs/CURVES_SPEC.md § 3`.

use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::DodoPmm;
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_dodo_quote_monotonic(
        b0 in 50_000u64..500_000u64,
        q0 in 50_000u64..500_000u64,
        oracle_int in 1u32..5u32,
        k_raw in 10u32..90u32,
        delta1 in 10u64..500u64,
        delta2 in 501u64..5_000u64,
    ) {
        let pool = DodoPmm::new(
            Decimal::from(b0),
            Decimal::from(q0),
            Decimal::from(b0),
            Decimal::from(q0),
            Decimal::from(oracle_int),
            Decimal::from(k_raw) / dec!(100),
            dec!(0.997),
            true,
        ).unwrap();

        let d1 = Decimal::from(delta1);
        let d2 = Decimal::from(delta2);

        let out1 = pool.quote(d1).unwrap();
        let out2 = pool.quote(d2).unwrap();

        prop_assert!(out2 >= out1, "out2 ({}) must be >= out1 ({})", out2, out1);
    }

    #[test]
    fn prop_dodo_optimal_sizing_tier_and_profit(
        b0 in 50_000u64..500_000u64,
        q0 in 50_000u64..500_000u64,
    ) {
        let pool = DodoPmm::new(
            Decimal::from(b0),
            Decimal::from(q0),
            Decimal::from(b0),
            Decimal::from(q0),
            dec!(2.0),
            dec!(0.5),
            dec!(0.997),
            true,
        ).unwrap();

        let ref_price = dec!(1.5);
        if let Ok(res) = pool.optimal_size(ref_price, SizingConstraints { fixed_cost: dec!(0), max_size: None }) {
            let is_empirically_validated = matches!(res.guarantee_tier, GuaranteeTier::EmpiricallyValidated { unimodality_confirmed: true });
            prop_assert!(is_empirically_validated);
            prop_assert!(res.optimal_delta > dec!(0));
            prop_assert!(res.expected_profit > dec!(0));
        }
    }
}
