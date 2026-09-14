use rust_decimal_macros::dec;
use sizing_core::curves::StableSwap;

fn main() {
    let balanced = StableSwap::new(
        vec![dec!(10_000_000), dec!(10_000_000), dec!(10_000_000)],
        dec!(100),
        dec!(0.997),
    )
    .unwrap();
    let (d, iters) = balanced.solve_d().unwrap();
    println!("balanced D = {d}, iterations = {iters}");

    let imbalanced = StableSwap::new(
        vec![dec!(15_000_000), dec!(8_000_000), dec!(9_500_000)],
        dec!(100),
        dec!(0.997),
    )
    .unwrap();
    let (d2, iters2) = imbalanced.solve_d().unwrap();
    println!("imbalanced D = {d2}, iterations = {iters2}");
}
