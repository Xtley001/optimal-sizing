//! Property-based tests for `Pmm`, per `docs/TESTING.md § pmm_proptest.rs`.

use proptest::prelude::*;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use sizing_core::curves::pmm::{GOLDEN_RATIO_INV, GOLDEN_SECTION_TOLERANCE};
use sizing_core::curves::Pmm;
use sizing_core::{PricingCurve, SizingAlgorithm, SizingConstraints};

fn reserve_strategy() -> impl Strategy<Value = Decimal> {
    (1_000.0f64..1e9).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

fn price_strategy() -> impl Strategy<Value = Decimal> {
    (1.0f64..10_000.0f64).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

fn k_strategy() -> impl Strategy<Value = Decimal> {
    (0.0000001f64..0.001f64).prop_map(|v| Decimal::from_f64_retain(v).unwrap())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_golden_section_matches_grid_search(
        base_reserve in reserve_strategy(),
        quote_multiplier in 0.5f64..1.0f64,
        oracle_price in price_strategy(),
        k in k_strategy(),
        price_discount in 0.5f64..0.999f64,
    ) {
        // Session 9 fix: quote_multiplier is now <= 1.0, not 1.0..5.0.
        // target_base_reserve = base_reserve * quote_multiplier <=
        // base_reserve whenever quote_multiplier <= 1.0, which makes
        // crossover_delta() = max(target - base_reserve, 0) exactly 0 —
        // deterministically pure basic-regime (concave), matching what
        // this comment already claimed the generator did. The old
        // 1.0..5.0 range gave crossover deltas up to 4x base_reserve,
        // landing squarely in the (real, analytically-derived-convex,
        // see curves/pmm.rs's check_unimodality doc comment)
        // reverse regime for most draws — previously slipping past the
        // pre-fix check_unimodality's insufficient "sign never flips"
        // test as a false positive; correctly rejected now that the
        // check also verifies the sign is non-positive (concave).
        let quote_reserve = base_reserve * oracle_price * Decimal::from_f64_retain(quote_multiplier).unwrap();
        let pool = Pmm::new(base_reserve, quote_reserve, oracle_price, k).unwrap();
        let reference_price = oracle_price * Decimal::from_f64_retain(price_discount).unwrap();

        // Session 9 fix: use the same search_upper_bound() method
        // optimal_size itself calls, not an independently re-derived
        // (and now-stale) formula — see search_upper_bound's own doc
        // comment in curves/pmm.rs.
        let upper = pool.search_upper_bound();

        prop_assume!(pool.check_unimodality((Decimal::ZERO, upper)));

        let constraints = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
        let result = pool.optimal_size(reference_price, constraints);

        if let Ok(r) = result {
            // Dense (10,000-point) grid search per TESTING.md.
            let n = 10_000u32;
            let mut best_delta = Decimal::ZERO;
            let mut best_profit = Decimal::MIN;
            for i in 0..=n {
                let delta = upper * Decimal::from(i) / Decimal::from(n);
                if let Ok(q) = pool.quote(delta) {
                    let profit = q - reference_price * delta;
                    if profit > best_profit {
                        best_profit = profit;
                        best_delta = delta;
                    }
                }
            }

            // Session 9 fix: the grid search itself only samples at
            // resolution `upper/n` — when the true optimum is small
            // relative to `upper` (common once pole-clipping shrinks
            // `upper` a lot while the optimum stays modest), that grid
            // step can be far coarser than a tight relative tolerance,
            // so the grid's own quantization noise alone can exceed a
            // tolerance sized only off `best_delta`. This isn't a
            // golden-section correctness issue — confirmed by hand for
            // the minimal failing case this generator fix surfaced
            // (base_reserve=1000, quote_multiplier=0.5, oracle_price=1,
            // k≈0.000676: diff≈0.042, half a grid step≈0.074, so the
            // discrepancy is smaller than the grid's own resolution).
            // Tolerance now also accounts for the grid step itself.
            let grid_step = upper / Decimal::from(n);
            let diff = (r.optimal_delta - best_delta).abs();
            let tolerance = (Decimal::new(1, 4) * best_delta.abs().max(Decimal::ONE)).max(grid_step * Decimal::TWO);
            prop_assert!(
                diff <= tolerance,
                "golden_section={} grid_search={} diff={} tolerance={}",
                r.optimal_delta, best_delta, diff, tolerance
            );
        }
    }

    #[test]
    fn prop_iteration_count_matches_formula(
        base_reserve in reserve_strategy(),
        oracle_price in price_strategy(),
        k in k_strategy(),
        price_discount in 0.5f64..0.999f64,
    ) {
        let quote_reserve = base_reserve * oracle_price;
        let pool = Pmm::new(base_reserve, quote_reserve, oracle_price, k).unwrap();
        let reference_price = oracle_price * Decimal::from_f64_retain(price_discount).unwrap();

        let constraints = SizingConstraints { fixed_cost: Decimal::ZERO, max_size: None };
        let result = pool.optimal_size(reference_price, constraints);

        if let Ok(r) = result {
            // Session 9 fix: call the same search_upper_bound() method
            // optimal_size itself now uses, rather than independently
            // re-deriving the bound here — this test previously
            // recomputed the old, now-removed `base_reserve * 1000`
            // formula, which had already silently drifted from what
            // optimal_size actually used to compute the expected
            // iteration count. It happened to keep passing anyway (the
            // iteration-count formula is log-based and not very
            // sensitive to the bound's exact value), but that was a
            // fragile coincidence, not a real guarantee — see
            // curves/pmm.rs's search_upper_bound doc comment.
            let l = pool.search_upper_bound(); // lo = 0

            let expected_n = ((GOLDEN_SECTION_TOLERANCE / l).ln() / GOLDEN_RATIO_INV.ln())
                .ceil()
                .to_u32()
                .unwrap_or(1)
                .max(1);

            let actual = r.iterations.expect("StableSwap/Pmm must report iterations");
            let diff = (actual as i64 - expected_n as i64).abs();
            prop_assert!(diff <= 1, "actual={actual} expected={expected_n}");
        }
    }
}

/// Not a proptest — a hand-constructed, non-random case known to violate
/// unimodality, per TESTING.md. Constructed from real Session 4 analysis:
/// a base-deficient pool (base_reserve well below the value implied by
/// quote_reserve at the oracle price) with a crossover point well inside
/// the reverse-regime pole. quote() then genuinely transitions from the
/// convex reverse-sell regime to the concave basic-sell regime mid-domain
/// — a real curvature sign flip check_unimodality is specifically designed
/// to catch (see the DECISION MADE note on check_unimodality in
/// curves/pmm.rs for why this is the reference-price-independent
/// sufficient condition being tested, and curves/pmm.rs's module-level
/// DECISION MADE note for the piecewise reverse/basic model itself).
#[test]
fn prop_unimodality_check_flags_known_bad_cases() {
    let oracle_price = dec!(2000);
    let k = dec!(0.0000001);
    // pole = 1/(k*oracle_price) = 5000

    let cases = [
        // (base_reserve, quote_reserve, label)
        (
            dec!(999_800),
            dec!(1_000_000) * oracle_price,
            "crossover=200, well under pole=5000",
        ),
        (
            dec!(998_000),
            dec!(1_000_000) * oracle_price,
            "crossover=2000, still under pole=5000",
        ),
    ];

    for (base_reserve, quote_reserve, label) in cases {
        let pool = Pmm::new(base_reserve, quote_reserve, oracle_price, k).unwrap();
        let confirmed = pool.check_unimodality((Decimal::ZERO, dec!(4900)));
        assert!(
            !confirmed,
            "expected check_unimodality to flag a violation for case '{label}' (base_reserve={base_reserve}), got true"
        );
    }

    // Control: a balanced pool (crossover = 0, single concave regime, no
    // curvature flip) must NOT be flagged — confirms the check isn't
    // trivially always false.
    let balanced = Pmm::new(
        dec!(1_000_000),
        dec!(1_000_000) * oracle_price,
        oracle_price,
        k,
    )
    .unwrap();
    assert!(balanced.check_unimodality((Decimal::ZERO, dec!(4900))));
}
