use rust_decimal_macros::dec;
use sizing_core::curves::Pmm;
use sizing_core::PricingCurve;

fn main() {
    // Balanced pool, base_reserve == target implied by quote_reserve/oracle_price
    // -> single (basic, concave) regime, no reverse-regime pole to worry about.
    let pool = Pmm::new(
        dec!(1_000_000),
        dec!(2_000_000_000),
        dec!(2000),
        dec!(0.0000001),
    )
    .unwrap();
    let out = pool.quote(dec!(10_000)).unwrap();
    println!("quote(10000) = {out}");
}
