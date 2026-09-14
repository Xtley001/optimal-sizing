//! Property-based tests for `Cpmm`, per `docs/TESTING.md § cpmm_proptest.rs`.

use proptest::prelude::*;
use rust_decimal::Decimal;
use sizing_core::curves::Cpmm;
use sizing_core::{PricingCurve, SizingAlgorithm, SizingConstraints};

/// Ranged decimal strategies per TESTING.md: x, y in [1, 1e12], f in
/// (0.9, 1.0], P in (0, 10]. proptest has no native Decimal strategy, so
/// these generate an f64 in range and convert — precision loss from f64
/// only affects which exact inputs get tested, not the Decimal math under
/// test, which runs entirely on the converted Decimal values.
fn reserve_strategy() -> impl Strategy<Value = Decimal> {
    (1.0f64..1e12).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

fn fee_strategy() -> impl Strategy<Value = Decimal> {
    (0.900001f64..=1.0f64).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

fn price_strategy() -> impl Strategy<Value = Decimal> {
    (0.000001f64..=10.0f64).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

fn no_arb_price_strategy(x: Decimal, y: Decimal, f: Decimal) -> Decimal {
    // Any P >= marginal price guarantees f*y/x <= P, i.e. no arbitrage.
    (f * y / x) + Decimal::ONE
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn prop_closed_form_matches_bisection(
        x in reserve_strategy(),
        y in reserve_strategy(),
        f in fee_strategy(),
        p in price_strategy(),
    ) {
        let pool = Cpmm::new(x, y, f).unwrap();
        let constraints = || SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };

        let closed_form = pool.optimal_size(p, constraints());
        let bisection = pool.bisection_fallback(p, constraints());

        // Both methods must agree on whether a profitable trade exists.
        prop_assert_eq!(closed_form.is_ok(), bisection.is_ok());

        if let (Ok(cf), Ok(bs)) = (closed_form, bisection) {
            // 1e-4 relative tolerance per TESTING.md.
            let diff = (cf.optimal_delta - bs.optimal_delta).abs();
            let tolerance = Decimal::new(1, 4) * cf.optimal_delta.abs().max(Decimal::ONE);
            prop_assert!(
                diff <= tolerance,
                "closed_form={} bisection={} diff={} tolerance={}",
                cf.optimal_delta, bs.optimal_delta, diff, tolerance
            );
        }
    }

    #[test]
    fn prop_global_optimum(
        x in reserve_strategy(),
        y in reserve_strategy(),
        f in fee_strategy(),
        p in price_strategy(),
    ) {
        let pool = Cpmm::new(x, y, f).unwrap();
        let constraints = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };

        if let Ok(result) = pool.optimal_size(p, constraints) {
            let optimal_delta = result.optimal_delta;
            let profit_at = |delta: Decimal| -> Option<Decimal> {
                pool.quote(delta).ok().map(|out| out - p * delta)
            };
            let profit_at_optimum = profit_at(optimal_delta).unwrap();

            for pct in [
                Decimal::new(99, 2),   // -1%
                Decimal::new(95, 2),   // -5%
                Decimal::new(90, 2),   // -10%
                Decimal::new(101, 2),  // +1%
                Decimal::new(105, 2),  // +5%
                Decimal::new(110, 2),  // +10%
            ] {
                let sample = optimal_delta * pct;
                if sample < Decimal::ZERO {
                    continue;
                }
                if let Some(profit_sample) = profit_at(sample) {
                    prop_assert!(
                        profit_sample <= profit_at_optimum,
                        "sample delta={} profit={} exceeds optimum profit={}",
                        sample, profit_sample, profit_at_optimum
                    );
                }
            }
        }
    }

    #[test]
    fn prop_concavity_holds(
        x in reserve_strategy(),
        y in reserve_strategy(),
        f in fee_strategy(),
    ) {
        let pool = Cpmm::new(x, y, f).unwrap();
        // Finite-difference second derivative of Profit at 10 sampled
        // points across the valid domain (0, x/f) — reference_price
        // cancels out of Profit'' since it's linear, so P is irrelevant
        // here and omitted, matching whitepaper.md § 5.1 eq. (4).
        let h = x * Decimal::new(1, 6); // small step relative to reserve scale
        let domain_bound = x / f;

        for i in 1..=10 {
            let delta = domain_bound * Decimal::new(i, 2); // 1%..10% into domain
            if delta <= h {
                continue;
            }
            let q = |d: Decimal| pool.quote(d).ok();
            if let (Some(q_minus), Some(q_mid), Some(q_plus)) =
                (q(delta - h), q(delta), q(delta + h))
            {
                let second_derivative = (q_plus - Decimal::TWO * q_mid + q_minus) / (h * h);
                prop_assert!(
                    second_derivative < Decimal::ZERO,
                    "Profit'' >= 0 at delta={}: {}",
                    delta, second_derivative
                );
            }
        }
    }

    #[test]
    fn prop_no_arbitrage_detected_correctly(
        x in reserve_strategy(),
        y in reserve_strategy(),
        f in fee_strategy(),
    ) {
        let pool = Cpmm::new(x, y, f).unwrap();
        let p = no_arb_price_strategy(x, y, f);
        let constraints = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };

        let result = pool.optimal_size(p, constraints);
        prop_assert!(matches!(result, Err(sizing_core::SizingError::NoArbitrageOpportunity)));
    }
}
