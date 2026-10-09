use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::{DlmmBin, LiquidityBook};
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints};

proptest! {
    #[test]
    fn prop_dlmm_quote_monotonic(
        delta1 in 10u32..500u32,
        delta2 in 501u32..1500u32,
    ) {
        let bins = vec![
            DlmmBin {
                bin_id: 10,
                price: dec!(1.10),
                reserve_x: dec!(0),
                reserve_y: dec!(1100),
            },
            DlmmBin {
                bin_id: 9,
                price: dec!(1.05),
                reserve_x: dec!(0),
                reserve_y: dec!(2100),
            },
            DlmmBin {
                bin_id: 8,
                price: dec!(1.00),
                reserve_x: dec!(0),
                reserve_y: dec!(3000),
            },
        ];

        let book = LiquidityBook::new(bins, 0, dec!(0.998)).unwrap();

        let q1 = book.quote(Decimal::from(delta1)).unwrap();
        let q2 = book.quote(Decimal::from(delta2)).unwrap();

        prop_assert!(q2 >= q1, "quote must be monotonically non-decreasing: q2={} < q1={}", q2, q1);
    }

    #[test]
    fn prop_dlmm_optimal_sizing_proven_optimal(
        fixed_cost in 1u32..20u32,
    ) {
        let bins = vec![
            DlmmBin {
                bin_id: 10,
                price: dec!(1.10),
                reserve_x: dec!(0),
                reserve_y: dec!(1100),
            },
            DlmmBin {
                bin_id: 9,
                price: dec!(1.05),
                reserve_x: dec!(0),
                reserve_y: dec!(2100),
            },
            DlmmBin {
                bin_id: 8,
                price: dec!(1.00),
                reserve_x: dec!(0),
                reserve_y: dec!(3000),
            },
        ];

        let book = LiquidityBook::new(bins, 0, dec!(1.0)).unwrap();

        // Reference price 1.02 clears bin 10 and bin 9, but not bin 8
        let constraints = SizingConstraints {
            fixed_cost: Decimal::from(fixed_cost),
            max_size: None,
        };

        let result = book.optimal_size(dec!(1.02), constraints).unwrap();
        // Capacity of bin 10 (1000) + bin 9 (2000) = 3000
        prop_assert_eq!(result.optimal_delta, dec!(3000));
        prop_assert_eq!(result.guarantee_tier, GuaranteeTier::ProvenOptimal);
        prop_assert_eq!(result.iterations, None);
    }
}
