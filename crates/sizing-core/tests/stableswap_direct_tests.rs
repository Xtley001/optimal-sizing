//! Direct coverage for `StableSwap::quote` / `StableSwap::optimal_size`,
//! satisfying `docs/TESTING.md § Coverage expectations`: "every public
//! function in API.md has at least one direct test... not merely covered
//! incidentally through another test." The named property tables in
//! `stableswap_proptest.rs` only exercise `new`/`solve_d`; these are
//! additive, not a substitute for or deviation from that table.

use rust_decimal_macros::dec;
use sizing_core::curves::StableSwap;
use sizing_core::{PricingCurve, SizingAlgorithm, SizingConstraints, SizingError};

#[test]
fn stableswap_quote_balanced_pool_near_one_to_one() {
    // On a balanced pool, a small trade should execute at close to 1:1,
    // minus fee — this is the defining property of a stable-asset curve
    // (whitepaper.md § 5.2 motivation) and a sanity check independent of
    // the regression's exact hard-coded digits.
    let pool = StableSwap::new(
        vec![dec!(10_000_000), dec!(10_000_000), dec!(10_000_000)],
        dec!(100),
        dec!(0.997),
    )
    .unwrap();

    let out = pool.quote(dec!(1_000)).unwrap();
    let relative_diff = (out - dec!(997)).abs() / dec!(997);
    assert!(
        relative_diff < dec!(0.001),
        "expected ~997 for a small balanced-pool trade, got {out}"
    );
}

#[test]
fn stableswap_quote_zero_delta_is_zero() {
    let pool = StableSwap::new(
        vec![dec!(10_000_000), dec!(10_000_000), dec!(10_000_000)],
        dec!(100),
        dec!(0.997),
    )
    .unwrap();

    let out = pool.quote(dec!(0)).unwrap();
    assert_eq!(out, dec!(0));
}

#[test]
fn stableswap_optimal_size_finds_profitable_trade_on_imbalanced_pool() {
    // Pool imbalanced toward reserves[0]: asset 1 is scarce relative to
    // asset 0, so a low-enough reference_price should be profitable to
    // arbitrage (buy the scarce asset from the pool).
    let pool = StableSwap::new(
        vec![dec!(15_000_000), dec!(8_000_000), dec!(9_500_000)],
        dec!(100),
        dec!(0.997),
    )
    .unwrap();

    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };
    let result = pool.optimal_size(dec!(0.5), constraints).unwrap();

    assert!(result.optimal_delta > dec!(0));
    assert!(result.expected_profit > dec!(0));
    assert!(result.iterations.is_some());
}

#[test]
fn stableswap_optimal_size_no_arbitrage_when_overpriced() {
    let pool = StableSwap::new(
        vec![dec!(15_000_000), dec!(8_000_000), dec!(9_500_000)],
        dec!(100),
        dec!(0.997),
    )
    .unwrap();

    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };
    let result = pool.optimal_size(dec!(2.0), constraints);

    assert_eq!(result, Err(SizingError::NoArbitrageOpportunity));
}
