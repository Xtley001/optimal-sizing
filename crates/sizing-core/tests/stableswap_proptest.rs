//! Property-based tests for `StableSwap`, per
//! `docs/TESTING.md § stableswap_proptest.rs`.

use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use sizing_core::curves::stableswap::{MAX_NEWTON_ITERATIONS, NEWTON_TOLERANCE};
use sizing_core::curves::StableSwap;
use sizing_core::SizingError;

/// Reserves in [1_000, 1e9], generated as f64 then converted — proptest has
/// no native Decimal strategy.
fn reserve_strategy() -> impl Strategy<Value = Decimal> {
    (1_000.0f64..1e9).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

/// Amplification in [1, 5000] per TESTING.md.
fn amplification_strategy() -> impl Strategy<Value = Decimal> {
    (1.0f64..=5000.0f64).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

/// D_P computed the same overflow-safe iterative way solve_d uses
/// internally, so this test can independently check eq. (6) without
/// re-deriving StableSwap's internals.
fn invariant_holds(reserves: &[Decimal], amplification: Decimal, d: Decimal) -> bool {
    let n = reserves.len();
    let n_dec = Decimal::from(n as u64);
    let n_n = n_dec.powu(n as u64);
    let s: Decimal = reserves.iter().copied().sum();

    let mut d_p = d;
    for reserve in reserves {
        d_p = d_p * d / (n_dec * reserve);
    }

    // eq. (6): A*n^n*S + D = A*D*n^n + D^(n+1)/(n^n*P_r)  [D^(n+1)/(n^n*P_r) == D_P]
    let lhs = amplification * n_n * s + d;
    let rhs = amplification * d * n_n + d_p;

    let relative_diff = (lhs - rhs).abs() / lhs.abs().max(Decimal::ONE);
    relative_diff < NEWTON_TOLERANCE
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn prop_newton_converges(
        r1 in reserve_strategy(),
        r2 in reserve_strategy(),
        extra in prop::option::of(reserve_strategy()),
        extra2 in prop::option::of(reserve_strategy()),
        amplification in amplification_strategy(),
    ) {
        let mut reserves = vec![r1, r2];
        if let Some(r) = extra { reserves.push(r); }
        if let Some(r) = extra2 { reserves.push(r); }

        let pool = StableSwap::new(reserves, amplification, Decimal::new(997, 3)).unwrap();
        let result = pool.solve_d();

        prop_assert!(result.is_ok());
        if let Ok((_, iterations)) = result {
            prop_assert!(iterations <= MAX_NEWTON_ITERATIONS);
        }
    }

    #[test]
    fn prop_invariant_satisfied(
        r1 in reserve_strategy(),
        r2 in reserve_strategy(),
        extra in prop::option::of(reserve_strategy()),
        amplification in amplification_strategy(),
    ) {
        let mut reserves = vec![r1, r2];
        if let Some(r) = extra { reserves.push(r); }

        let pool = StableSwap::new(reserves.clone(), amplification, Decimal::new(997, 3)).unwrap();
        if let Ok((d, _)) = pool.solve_d() {
            prop_assert!(invariant_holds(&reserves, amplification, d));
        }
    }

    #[test]
    fn prop_convergence_failure_reported(
        r1 in reserve_strategy(),
        r2 in reserve_strategy(),
        amplification in amplification_strategy(),
    ) {
        // amplification exactly 0 — constructed directly via StableSwap::new,
        // must be rejected at construction time, not during Newton's method.
        let zero_amp_result = StableSwap::new(vec![r1, r2], Decimal::ZERO, Decimal::new(997, 3));
        let zero_amp_is_invalid = matches!(zero_amp_result, Err(SizingError::InvalidReserves { .. }));
        prop_assert!(zero_amp_is_invalid);

        // a reserve set to 0 — constructed directly via StableSwap::new,
        // must be rejected at construction time.
        let zero_reserve_result =
            StableSwap::new(vec![r1, Decimal::ZERO], amplification, Decimal::new(997, 3));
        let zero_reserve_is_invalid =
            matches!(zero_reserve_result, Err(SizingError::InvalidReserves { .. }));
        prop_assert!(zero_reserve_is_invalid);
    }
}
