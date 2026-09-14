use rust_decimal_macros::dec;
use sizing_core::curves::Cpmm;
use sizing_core::{SizingAlgorithm, SizingConstraints};

fn main() {
    let pool = Cpmm::new(dec!(1_200_000), dec!(800_000), dec!(0.997)).unwrap();
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };
    let result = pool.optimal_size(dec!(0.60), constraints).unwrap();
    println!("optimal_delta = {}", result.optimal_delta);
    println!("expected_profit = {}", result.expected_profit);

    let no_arb = pool.optimal_size(
        dec!(0.6725),
        SizingConstraints {
            fixed_cost: dec!(0),
            max_size: None,
        },
    );
    println!("no_arb result = {:?}", no_arb.is_err());
}
