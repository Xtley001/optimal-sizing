//! Property-based tests for Curve CryptoSwap v2.
//! Per `docs/TESTING.md` and `docs/CURVES_SPEC.md § 4`.

use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::CurveCryptoSwap;
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_cryptoswap_quote_monotonic(
        x0 in 500_000u64..2_000_000u64,
        x1 in 500_000u64..2_000_000u64,
        delta1 in 10u64..500u64,
        delta2 in 501u64..5_000u64,
    ) {
        let pool = CurveCryptoSwap::new(
            [Decimal::from(x0), Decimal::from(x1)],
            dec!(100),
            dec!(0.0001),
            dec!(0.997),
            0,
        ).unwrap();

        let d1 = Decimal::from(delta1);
        let d2 = Decimal::from(delta2);

        let out1 = pool.quote(d1).unwrap();
        let out2 = pool.quote(d2).unwrap();

        prop_assert!(out2 >= out1, "out2 ({}) must be >= out1 ({})", out2, out1);
    }

    #[test]
    fn prop_cryptoswap_optimal_sizing_tier_and_profit(
        x0 in 500_000u64..2_000_000u64,
        x1 in 500_000u64..2_000_000u64,
    ) {
        let pool = CurveCryptoSwap::new(
            [Decimal::from(x0), Decimal::from(x1)],
            dec!(100),
            dec!(0.0001),
            dec!(0.997),
            0,
        ).unwrap();

        let ref_price = dec!(0.9);
        if let Ok(res) = pool.optimal_size(ref_price, SizingConstraints { fixed_cost: dec!(0), max_size: None }) {
            let is_numerically_guaranteed = matches!(res.guarantee_tier, GuaranteeTier::NumericallyGuaranteed { convergence_conditions_met: true });
            prop_assert!(is_numerically_guaranteed);
            prop_assert!(res.optimal_delta > dec!(0));
            prop_assert!(res.expected_profit > dec!(0));
        }
    }
}
