//! Property-based tests for heterogeneous multi-hop routing.
//! Per `docs/TESTING.md` and `docs/ROUTING_SPEC.md § 1 & § 2`.

use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::{BalancerWeightedPool, ConcentratedLiquidity, Cpmm};
use sizing_core::types::{GuaranteeTier, SizingConstraints};
use sizing_router::route::{Leg, Route};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn prop_heterogeneous_route_quote_monotonic(
        r1_in in 500_000u64..2_000_000u64,
        r1_out in 500_000u64..2_000_000u64,
        delta1 in 10u64..500u64,
        delta2 in 501u64..2_000u64,
    ) {
        let leg1 = Leg::Cpmm(Cpmm::new(Decimal::from(r1_in), Decimal::from(r1_out), dec!(0.997)).unwrap());
        let leg2 = Leg::BalancerWeighted(
            BalancerWeightedPool::new(
                vec![dec!(2_000_000), dec!(1_500_000)],
                vec![dec!(0.5), dec!(0.5)],
                dec!(0.997),
                0,
                1,
            ).unwrap()
        );
        let leg3 = Leg::ConcentratedLiquidity(
            ConcentratedLiquidity::new(
                dec!(5_000_000),
                dec!(0.5),
                dec!(2.0),
                dec!(1.0),
                dec!(0.997),
            ).unwrap()
        );

        let route = Route::new_multi_hop(vec![leg1, leg2, leg3]).unwrap();

        let d1 = Decimal::from(delta1);
        let d2 = Decimal::from(delta2);

        if let (Some(out1), Some(out2)) = (route.route_output(d1), route.route_output(d2)) {
            prop_assert!(out2 >= out1, "out2 ({}) must be >= out1 ({})", out2, out1);
        }
    }

    #[test]
    fn prop_heterogeneous_route_optimal_sizing(
        r1_in in 500_000u64..2_000_000u64,
        r1_out in 1_000_000u64..3_000_000u64,
    ) {
        let leg1 = Leg::Cpmm(Cpmm::new(Decimal::from(r1_in), Decimal::from(r1_out), dec!(0.997)).unwrap());
        let leg2 = Leg::BalancerWeighted(
            BalancerWeightedPool::new(
                vec![dec!(2_000_000), dec!(1_500_000)],
                vec![dec!(0.5), dec!(0.5)],
                dec!(0.997),
                0,
                1,
            ).unwrap()
        );

        let route = Route::new_multi_hop(vec![leg1, leg2]).unwrap();
        let ref_price = dec!(0.5);

        if let Ok(res) = route.optimal_size(ref_price, SizingConstraints { fixed_cost: dec!(0), max_size: None }) {
            prop_assert!(res.optimal_delta > dec!(0));
            prop_assert!(res.expected_profit > dec!(0));
            if let GuaranteeTier::ComposedFrom(tiers) = res.guarantee_tier {
                prop_assert_eq!(tiers.len(), 2);
            } else {
                prop_assert!(false, "expected ComposedFrom tier");
            }
        }
    }
}
