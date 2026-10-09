use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::VelodromeStable;
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints};

proptest! {
    #[test]
    fn prop_velodrome_quote_strictly_monotonic(
        x in 100_000u32..500_000u32,
        y in 100_000u32..500_000u32,
        delta1 in 10u32..100u32,
        delta2 in 101u32..300u32,
    ) {
        let pool = VelodromeStable::new(
            Decimal::from(x),
            Decimal::from(y),
            dec!(0.9995),
        ).unwrap();

        let q1 = pool.quote(Decimal::from(delta1)).unwrap();
        let q2 = pool.quote(Decimal::from(delta2)).unwrap();

        prop_assert!(q2 > q1, "quote must be strictly monotonic: q2={} <= q1={}", q2, q1);
    }

    #[test]
    fn prop_velodrome_optimal_sizing_tier_and_profit(
        x in 500_000u32..1_000_000u32,
        y in 500_000u32..1_000_000u32,
    ) {
        let pool = VelodromeStable::new(
            Decimal::from(x),
            Decimal::from(y),
            dec!(0.9995),
        ).unwrap();

        let marginal_zero = pool.marginal_price_at_zero();
        let profitable_ref_price = marginal_zero * dec!(0.95);

        let constraints = SizingConstraints {
            fixed_cost: dec!(10),
            max_size: None,
        };

        let result = pool.optimal_size(profitable_ref_price, constraints).unwrap();
        prop_assert!(result.optimal_delta > Decimal::ZERO);
        prop_assert!(result.expected_profit > dec!(10));
        prop_assert_eq!(
            result.guarantee_tier,
            GuaranteeTier::NumericallyGuaranteed { convergence_conditions_met: true }
        );
    }
}
