use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::Cpmm;
use sizing_core::types::SizingConstraints;
use sizing_portfolio::{PortfolioOpportunity, PortfolioSizer};

proptest! {
    #[test]
    fn prop_portfolio_never_exceeds_budget(
        budget in 5_000u32..50_000u32,
    ) {
        let pool1 = Cpmm::new(dec!(1_000_000), dec!(1_000_000), dec!(0.997)).unwrap();
        let pool2 = Cpmm::new(dec!(1_500_000), dec!(1_500_000), dec!(0.997)).unwrap();

        let opps = vec![
            PortfolioOpportunity {
                id: "pool1".to_string(),
                pool: Box::new(pool1),
                reference_price: dec!(0.95),
                constraints: SizingConstraints {
                    fixed_cost: dec!(1),
                    max_size: None,
                },
            },
            PortfolioOpportunity {
                id: "pool2".to_string(),
                pool: Box::new(pool2),
                reference_price: dec!(0.96),
                constraints: SizingConstraints {
                    fixed_cost: dec!(1),
                    max_size: None,
                },
            },
        ];

        let budget_dec = Decimal::from(budget);
        let sizer = PortfolioSizer::new(budget_dec).unwrap();
        let result = sizer.optimize(&opps).unwrap();

        prop_assert!(
            result.total_capital_used <= budget_dec + dec!(1),
            "allocated capital {} exceeded budget {}",
            result.total_capital_used,
            budget_dec
        );
    }
}
