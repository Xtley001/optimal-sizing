//! Direct coverage for `Pmm::quote` / `Pmm::optimal_size`, satisfying
//! `docs/TESTING.md § Coverage expectations`. The named property tables in
//! `pmm_proptest.rs` exercise `check_unimodality`/`optimal_size`'s search
//! machinery but not `quote` directly, or the plain existence/error paths;
//! these are additive.

use rust_decimal_macros::dec;
use sizing_core::curves::Pmm;
use sizing_core::{PricingCurve, SizingAlgorithm, SizingConstraints, SizingError};

#[test]
fn pmm_quote_zero_delta_is_zero() {
    let pool = Pmm::new(
        dec!(1_000_000),
        dec!(2_000_000_000),
        dec!(2000),
        dec!(0.0000001),
    )
    .unwrap();
    assert_eq!(pool.quote(dec!(0)).unwrap(), dec!(0));
}

#[test]
fn pmm_quote_balanced_pool_near_oracle_price_for_small_trade() {
    // Balanced pool, small trade -> output should be close to delta * oracle_price.
    let pool = Pmm::new(
        dec!(1_000_000),
        dec!(2_000_000_000),
        dec!(2000),
        dec!(0.0000001),
    )
    .unwrap();
    let out = pool.quote(dec!(1)).unwrap();
    let relative_diff = (out - dec!(2000)).abs() / dec!(2000);
    assert!(
        relative_diff < dec!(0.01),
        "expected ~2000 for a tiny trade, got {out}"
    );
}

#[test]
fn pmm_optimal_size_no_arbitrage_when_reference_price_exceeds_oracle_price() {
    let pool = Pmm::new(
        dec!(1_000_000),
        dec!(2_000_000_000),
        dec!(2000),
        dec!(0.0000001),
    )
    .unwrap();
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };
    let result = pool.optimal_size(dec!(2001), constraints);
    assert_eq!(result, Err(SizingError::NoArbitrageOpportunity));
}

#[test]
fn pmm_optimal_size_finds_profitable_trade_when_underpriced() {
    let pool = Pmm::new(
        dec!(1_000_000),
        dec!(2_000_000_000),
        dec!(2000),
        dec!(0.0000001),
    )
    .unwrap();
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };
    let result = pool.optimal_size(dec!(1900), constraints).unwrap();

    assert!(result.optimal_delta > dec!(0));
    assert!(result.expected_profit > dec!(0));
    assert!(result.iterations.is_some());
}

#[test]
fn pmm_new_rejects_invalid_parameters() {
    assert!(matches!(
        Pmm::new(dec!(0), dec!(1000), dec!(1), dec!(0.001)),
        Err(SizingError::InvalidReserves { .. })
    ));
    assert!(matches!(
        Pmm::new(dec!(1000), dec!(0), dec!(1), dec!(0.001)),
        Err(SizingError::InvalidReserves { .. })
    ));
    assert!(matches!(
        Pmm::new(dec!(1000), dec!(1000), dec!(0), dec!(0.001)),
        Err(SizingError::InvalidReserves { .. })
    ));
    assert!(matches!(
        Pmm::new(dec!(1000), dec!(1000), dec!(1), dec!(0)),
        Err(SizingError::InvalidReserves { .. })
    ));
}

/// Session 9 regression: `reverse_sell_base(Δ) = Δ*p/(1-k*Δ*p)` is
/// analytically convex (f''(Δ) = 2*k*p²/(1-k*Δ*p)³ > 0 for k, p > 0 in
/// its valid domain), not concave. Before this session's fix,
/// `check_unimodality` only checked that curvature never *flipped* sign
/// — a search interval confined entirely to the reverse regime (never
/// reaching the basic-regime crossover) has curvature that's
/// consistently positive (never flips), so it was incorrectly reported
/// as `unimodality_confirmed: true`. This constructs exactly that case
/// (crossover_delta well beyond the search interval's own upper bound,
/// so the interval never leaves the reverse regime) and checks the fix
/// directly, independent of the pmm_proptest.rs generator fix.
#[test]
fn reverse_regime_only_interval_is_correctly_flagged_as_non_concave() {
    let base_reserve = dec!(100);
    let quote_reserve = dec!(1_000_000);
    let oracle_price = dec!(2000);
    let k = dec!(0.0000001);
    // target_base_reserve = quote_reserve / oracle_price = 500
    // crossover_delta = 500 - 100 = 400
    // pole = 1 / (k * oracle_price) = 1 / 0.0002 = 5000
    let pool = Pmm::new(base_reserve, quote_reserve, oracle_price, k).unwrap();

    // Interval [0, 300] stays entirely inside the reverse regime
    // (crossover is at 400, well beyond this interval's own upper bound)
    // and well short of the pole at 5000 — purely convex throughout,
    // never touching the basic (concave) regime at all.
    let confirmed = pool.check_unimodality((rust_decimal::Decimal::ZERO, dec!(300)));
    assert!(
        !confirmed,
        "a purely-convex reverse-regime-only interval must not be reported as unimodality_confirmed: true"
    );

    // Control: a genuinely concave-only interval (pure basic regime,
    // balanced pool, no reverse regime at all) must still pass — the fix
    // shouldn't have made the check trivially always-false.
    let balanced = Pmm::new(
        dec!(1_000_000),
        dec!(1_000_000) * oracle_price,
        oracle_price,
        k,
    )
    .unwrap();
    assert!(balanced.check_unimodality((rust_decimal::Decimal::ZERO, dec!(300))));
}
