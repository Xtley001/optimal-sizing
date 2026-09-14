//! Newton's method on the StableSwap invariant. Struct/signatures per
//! `docs/API.md § curves/stableswap.rs`. `solve_d` derivation in
//! `docs/whitepaper.md § 5.2` (Curve's own `get_D` algorithm, cited not
//! claimed as novel).

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;

use crate::error::SizingError;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{SizingConstraints, SizingResult};

/// Test-only instrumentation (Session 9): counts real calls to
/// `solve_d`'s Newton's-method solve, so the performance fix below can
/// be *measured*, not just asserted in a comment — see
/// `solve_d_call_count_drops_from_dozens_to_one_after_memoization` at
/// the bottom of this file.
#[cfg(test)]
pub(crate) static SOLVE_D_CALL_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// An `n`-asset StableSwap pool. Sizing against this curve is Newton's-
/// method-solved with stated convergence conditions — see
/// `whitepaper.md § 5.2`.
#[derive(Debug, Clone, PartialEq)]
pub struct StableSwap {
    /// Reserves of all `n` assets, length `n`, all `> 0`.
    pub reserves: Vec<Decimal>,
    /// Amplification coefficient ("A"), `>= 1`.
    pub amplification: Decimal,
    /// Fee retention, e.g. `dec!(0.997)` for a 30bps fee.
    pub fee_retention: Decimal,
}

/// Maximum Newton's-method iterations for `solve_d`, matching Curve's own
/// contract implementation.
pub const MAX_NEWTON_ITERATIONS: u32 = 255;
/// Relative convergence tolerance for Newton's method (1e-6).
pub const NEWTON_TOLERANCE: Decimal = dec!(0.000001);

impl StableSwap {
    /// Returns SizingError::InvalidReserves if any reserve <= 0,
    /// amplification < 1 (whitepaper.md § 5.2 regularity condition (b)),
    /// fee_retention not in (0, 1] (same domain as Cpmm's fee_retention —
    /// same field name/semantics, no separate constraint given), or
    /// reserves.len() < 2 (an n-asset invariant is degenerate below n=2).
    pub fn new(
        reserves: Vec<Decimal>,
        amplification: Decimal,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError> {
        if reserves.len() < 2 {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "reserves must contain at least 2 assets, got {}",
                    reserves.len()
                ),
            });
        }
        for (i, r) in reserves.iter().enumerate() {
            if *r <= Decimal::ZERO {
                return Err(SizingError::InvalidReserves {
                    detail: format!("reserves[{i}] must be > 0, got {r}"),
                });
            }
        }
        if amplification < Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("amplification must be >= 1, got {amplification}"),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }
        Ok(Self {
            reserves,
            amplification,
            fee_retention,
        })
    }

    /// Re-checks whitepaper.md § 5.2 regularity conditions (a)-(c) against
    /// `self` directly, rather than assuming them from construction-time
    /// validation. Today this can only ever return `true` for a `self`
    /// that exists at all — `StableSwap::new` already rejects `A < 1` and
    /// non-positive reserves, and `solve_d`'s `D_0 = S` starting point is
    /// unconditional — so this is currently a restatement, not a new
    /// finding. It exists so `GuaranteeTier::NumericallyGuaranteed`'s
    /// `convergence_conditions_met` is a real per-call computation against
    /// `self`'s actual fields rather than a hardcoded `true` literal, which
    /// matters if `StableSwap`'s constructor invariants are ever loosened
    /// (e.g. a future relaxed-construction path) without this call site
    /// being updated to match.
    fn regularity_conditions_met(&self) -> bool {
        self.amplification >= Decimal::ONE && self.reserves.iter().all(|r| *r > Decimal::ZERO)
    }

    /// Solves for the invariant D via Newton's method (whitepaper.md § 5.2
    /// eq. 7), starting from D_0 = S per regularity condition (c). Exposed
    /// publicly because callers building on top of this crate frequently
    /// need D independently of a sizing call (e.g. for LP share pricing).
    pub fn solve_d(&self) -> Result<(Decimal, u32), SizingError> {
        #[cfg(test)]
        SOLVE_D_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let n = self.reserves.len();
        let n_dec = Decimal::from(n as u64);
        let n_n = n_dec.powu(n as u64);
        let s: Decimal = self.reserves.iter().copied().sum();

        let mut d = s; // D_0 = S, regularity condition (c)

        for iteration in 1..=MAX_NEWTON_ITERATIONS {
            // D_P = D^(n+1) / (n^n * P_r), computed iteratively (interleaving
            // each multiply with its divide) rather than via a single
            // D^(n+1) power — the direct form overflows Decimal's ~28-29
            // digit range for realistic reserve magnitudes even though the
            // resulting D_P value itself is small. Mathematically identical
            // to eq. (7); this is the standard numerically-safe technique
            // (also used by Curve's own get_D contract code).
            let mut d_p = d;
            for reserve in &self.reserves {
                d_p = d_p * d / (n_dec * reserve);
            }

            let numerator = (self.amplification * n_n * s + d_p * n_dec) * d;
            let denominator =
                (self.amplification * n_n - Decimal::ONE) * d + (n_dec + Decimal::ONE) * d_p;

            if denominator == Decimal::ZERO {
                return Err(SizingError::DidNotConverge {
                    iterations_attempted: iteration,
                });
            }

            let d_next = numerator / denominator;
            let diff = (d_next - d).abs();
            let relative = if d.is_zero() { diff } else { diff / d.abs() };
            d = d_next;

            if relative < NEWTON_TOLERANCE {
                return Ok((d, iteration));
            }
        }

        Err(SizingError::DidNotConverge {
            iterations_attempted: MAX_NEWTON_ITERATIONS,
        })
    }
    /// Newton's-method solve for the new reserve of the output asset
    /// (index 1, see DECISION MADE below) after the input asset (index 0)
    /// reserve moves to `new_input_reserve`, holding D and all other
    /// reserves fixed. This is structurally the same Newton iteration as
    /// `solve_d` (whitepaper.md § 5.2), applied to a different unknown —
    /// the standard Curve `get_y` algorithm (cited, not novel, same
    /// treatment as `get_D`).
    ///
    /// DECISION MADE (this project's stated decision-logging discipline, logged Session 3): neither API.md nor
    /// whitepaper.md name explicit input/output asset indices for
    /// StableSwap's n-asset PricingCurve::quote. Adopting the convention
    /// reserves[0] = input asset, reserves[1] = output asset (mirroring
    /// Cpmm's x=input/y=output field ordering); reserves[2..] are other
    /// pool assets held fixed. This needs to land in API.md.
    fn solve_new_output_reserve(
        &self,
        d: Decimal,
        new_input_reserve: Decimal,
    ) -> Result<Decimal, SizingError> {
        let n = self.reserves.len();
        let n_dec = Decimal::from(n as u64);
        let ann = self.amplification * n_dec.powu(n as u64);

        let mut c = d;
        let mut s_ = Decimal::ZERO;
        for (k, &reserve) in self.reserves.iter().enumerate() {
            if k == 1 {
                continue; // j — the unknown we're solving for
            }
            let x_k = if k == 0 { new_input_reserve } else { reserve };
            s_ += x_k;
            c = c * d / (x_k * n_dec);
        }
        c = c * d / (ann * n_dec);
        let b = s_ + d / ann;

        let mut y = d;
        for iteration in 1..=MAX_NEWTON_ITERATIONS {
            let y_prev = y;
            let denom = Decimal::TWO * y + b - d;
            if denom == Decimal::ZERO {
                return Err(SizingError::DidNotConverge {
                    iterations_attempted: iteration,
                });
            }
            y = (y_prev * y_prev + c) / denom;
            let diff = (y - y_prev).abs();
            let relative = if y_prev.is_zero() {
                diff
            } else {
                diff / y_prev.abs()
            };
            if relative < NEWTON_TOLERANCE {
                return Ok(y);
            }
        }
        Err(SizingError::DidNotConverge {
            iterations_attempted: MAX_NEWTON_ITERATIONS,
        })
    }

    /// Same as `quote`, but takes an already-solved `d` instead of
    /// re-deriving it via `solve_d` — see `docs/ARCHITECTURE.md §
    /// StableSwap performance fix (Session 9)` for why this exists.
    /// Private: only safe to reuse a `d` value within the single
    /// immutable borrow of `self` it was computed from — Rust's borrow
    /// checker guarantees `self.reserves`/`self.amplification` cannot
    /// change during that borrow, so this is provably not a staleness
    /// risk *within one call*, but `d` must never be cached on `self` or
    /// reused across separate calls, since `reserves`/`amplification`
    /// are public and mutable between calls.
    fn quote_with_d(&self, delta_in: Decimal, d: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        let new_input_reserve = self.reserves[0] + delta_in;
        let new_output_reserve = self.solve_new_output_reserve(d, new_input_reserve)?;
        let raw_output = self.reserves[1] - new_output_reserve;
        let raw_output = if raw_output < Decimal::ZERO {
            // Newton's-method residual noise: solve_new_output_reserve only
            // converges to within NEWTON_TOLERANCE relative precision, so a
            // hair-negative raw_output near delta_in = 0 is expected noise,
            // not a genuinely invalid state. Only a meaningfully negative
            // result (beyond that tolerance) is a real error.
            let relative_magnitude = raw_output.abs() / self.reserves[1];
            if relative_magnitude < NEWTON_TOLERANCE {
                Decimal::ZERO
            } else {
                return Err(SizingError::InvalidReserves {
                    detail: format!("computed negative raw output {raw_output}"),
                });
            }
        } else {
            raw_output
        };
        Ok(raw_output * self.fee_retention)
    }
}

impl PricingCurve for StableSwap {
    /// Post-fee output for input `delta_in` on the input asset (index 0),
    /// swapped for the output asset (index 1) — see the DECISION MADE note
    /// on `solve_new_output_reserve` for the index convention.
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        let (d, _) = self.solve_d()?;
        self.quote_with_d(delta_in, d)
    }
}

impl SizingAlgorithm for StableSwap {
    /// Root-finds the profit-maximizing delta_in via bracket-and-bisect on
    /// the sign of Profit'(delta_in), where the derivative is estimated by
    /// central finite difference on `quote` — whitepaper.md § 5.2 specifies
    /// this is "the same Newton's-method machinery... applied to the sizing
    /// equation directly" but does not give the exact iteration (see
    /// Session 3 handoff).
    ///
    /// **Session 9 performance fix:** `D` (from `solve_d`, itself a
    /// 255-iteration-capped Newton's-method solve) depends only on
    /// `self.reserves`/`self.amplification`, not on the trade size being
    /// quoted — but `quote()` re-derives it from scratch on every call.
    /// This function's own `derivative` closure was calling `quote()`
    /// twice per evaluation, tens of times across the doubling and
    /// bisection phases, meaning `D` was being independently re-solved
    /// 40-80+ times to answer one `optimal_size()` call. `D` is now
    /// solved exactly once here and threaded through via `quote_with_d`
    /// instead — see that method's doc comment for why this is safe
    /// (the borrow checker guarantees `self` can't change mid-call) and
    /// why `D` still isn't cached as a struct field (it would go stale
    /// across separate calls, since `reserves`/`amplification` are public
    /// and mutable). `docs/ARCHITECTURE.md § StableSwap performance fix`
    /// has the full writeup.
    ///
    /// **Session 9 numerical fix:** the finite-difference step was
    /// previously scaled to `NEWTON_TOLERANCE` (1e-6) directly — the same
    /// relative scale as `quote()`'s own convergence noise floor, which
    /// risks catastrophic cancellation (subtracting two values that are
    /// each individually noisy at ~1e-6 relative precision, over a gap
    /// that's itself only ~1e-6 wide). The step is now scaled to
    /// `sqrt(NEWTON_TOLERANCE)` (~1e-3) instead — the standard rule of
    /// thumb for a finite-difference step relative to a function's own
    /// noise floor — trading a little truncation error for a lot less
    /// cancellation error. `docs/ARCHITECTURE.md` has the full reasoning;
    /// an analytic derivative of `solve_new_output_reserve`'s implicit
    /// equation would remove this tradeoff entirely but is a larger,
    /// separately-scoped change not attempted this session.
    ///
    /// fixed_cost/capacity handling per whitepaper.md § 5.4 (Session 5):
    /// same pattern as Cpmm — restrict_domain clips to max_size, and the
    /// fixed_cost threshold is checked separately here afterward.
    fn optimal_size(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError> {
        if reference_price <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("reference_price must be > 0, got {reference_price}"),
            });
        }

        // Per API.md: a SizingResult is never returned with
        // convergence_conditions_met: false — if the regularity conditions
        // don't hold, DidNotConverge is returned instead of a result
        // carrying a false guarantee. Unreachable today given
        // StableSwap::new's own validation, but checked for real here (not
        // assumed) so this stays correct if construction is ever loosened.
        if !self.regularity_conditions_met() {
            return Err(SizingError::DidNotConverge {
                iterations_attempted: 0,
            });
        }

        // Solved once, reused for every quote_with_d call below — see
        // this function's doc comment for why that's safe.
        let (d, _) = self.solve_d()?;

        // sqrt(NEWTON_TOLERANCE), not NEWTON_TOLERANCE itself — see this
        // function's doc comment. .sqrt() on a MathematicalOps Decimal
        // returns Option; NEWTON_TOLERANCE is a positive constant so this
        // is infallible in practice, but a real fallback (not a panic) is
        // still given for defense in depth.
        let step_scale = NEWTON_TOLERANCE.sqrt().unwrap_or(dec!(0.001));
        let step = self.reserves[0] * step_scale;

        let derivative = |delta: Decimal| -> Result<Decimal, SizingError> {
            let lo = if delta > step {
                delta - step
            } else {
                Decimal::ZERO
            };
            let hi = delta + step;
            let width = hi - lo;
            if width <= Decimal::ZERO {
                return Ok(-reference_price);
            }
            let q_lo = self.quote_with_d(lo, d)?;
            let q_hi = self.quote_with_d(hi, d)?;
            Ok((q_hi - q_lo) / width - reference_price)
        };

        let marginal = derivative(Decimal::ZERO)?;
        if marginal <= Decimal::ZERO {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let mut lo = Decimal::ZERO;
        let mut hi = step;
        let mut iterations = 0u32;

        let mut doubling_guard = 0u32;
        while derivative(hi)? > Decimal::ZERO {
            hi *= Decimal::TWO;
            doubling_guard += 1;
            iterations += 1;
            if doubling_guard > MAX_NEWTON_ITERATIONS {
                return Err(SizingError::DidNotConverge {
                    iterations_attempted: iterations,
                });
            }
        }

        while hi - lo > NEWTON_TOLERANCE * hi {
            let mid = (lo + hi) / Decimal::TWO;
            iterations += 1;
            if iterations > MAX_NEWTON_ITERATIONS {
                return Err(SizingError::DidNotConverge {
                    iterations_attempted: iterations,
                });
            }
            if derivative(mid)? > Decimal::ZERO {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let unconstrained_delta = (lo + hi) / Decimal::TWO;

        let (_, optimal_delta) = crate::profit::NetProfit::restrict_domain(
            &constraints,
            (Decimal::ZERO, unconstrained_delta),
        )?;

        let output = self.quote_with_d(optimal_delta, d)?;
        let gross_profit = output - reference_price * optimal_delta;
        let expected_profit = gross_profit - constraints.fixed_cost;
        if expected_profit <= Decimal::ZERO {
            return Err(SizingError::NoProfitableSize);
        }

        Ok(SizingResult {
            optimal_delta,
            expected_profit,
            guarantee_tier: crate::types::GuaranteeTier::NumericallyGuaranteed {
                // Guaranteed true at this point (checked upfront above,
                // which would have returned Err otherwise) — kept as a
                // real method call rather than a literal for defense in
                // depth if the upfront check is ever refactored away.
                convergence_conditions_met: self.regularity_conditions_met(),
            },
            iterations: Some(iterations),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::sync::atomic::Ordering;

    /// Proves the Session 9 performance fix actually does what its doc
    /// comment claims, rather than trusting the claim: before the fix,
    /// `solve_d` (a 255-iteration-capped Newton's-method solve) was
    /// re-run from scratch on every `quote()` call, and `optimal_size`'s
    /// derivative closure called `quote()` twice per evaluation, tens of
    /// times — dozens of redundant solves per `optimal_size` call. This
    /// test calls `optimal_size` exactly once and asserts `solve_d` was
    /// only actually run once, not dozens of times.
    #[test]
    fn solve_d_call_count_drops_from_dozens_to_one_after_memoization() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let pool = StableSwap::new(
            vec![dec!(15_000_000), dec!(8_000_000), dec!(9_500_000)],
            dec!(100),
            dec!(0.997),
        )
        .unwrap();

        SOLVE_D_CALL_COUNT.store(0, Ordering::SeqCst);
        let result = pool.optimal_size(
            dec!(0.9),
            SizingConstraints {
                fixed_cost: dec!(0),
                max_size: None,
            },
        );
        assert!(
            result.is_ok(),
            "sizing call itself should still succeed: {result:?}"
        );

        let calls = SOLVE_D_CALL_COUNT.load(Ordering::SeqCst);
        assert_eq!(
            calls, 1,
            "solve_d should be called exactly once per optimal_size call now that D is memoized within the call, got {calls} calls"
        );
    }

    /// Same proof from the other direction: a standalone `quote()` call
    /// (outside `optimal_size`) still solves `D` fresh every time, since
    /// it has no way to know reserves haven't changed since a previous
    /// call — confirming the fix didn't introduce any hidden persistent
    /// caching that could go stale.
    #[test]
    fn standalone_quote_calls_still_each_solve_d_independently() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let pool = StableSwap::new(
            vec![dec!(1_000_000), dec!(1_000_000), dec!(1_000_000)],
            dec!(100),
            dec!(0.997),
        )
        .unwrap();

        SOLVE_D_CALL_COUNT.store(0, Ordering::SeqCst);
        let _ = pool.quote(dec!(1000)).unwrap();
        let _ = pool.quote(dec!(2000)).unwrap();
        let _ = pool.quote(dec!(3000)).unwrap();

        let calls = SOLVE_D_CALL_COUNT.load(Ordering::SeqCst);
        assert_eq!(
            calls, 3,
            "each standalone quote() call should independently solve D, got {calls} calls"
        );
    }
}

#[cfg(test)]
mod perf_measurement {
    use super::*;
    use rust_decimal_macros::dec;
    use std::time::Instant;

    /// Not a correctness assertion — a one-off measurement (Session 9)
    /// to put a real number on the fix, printed with --nocapture, rather
    /// than just asserting "faster" without evidence.
    ///
    /// **Correction, logged honestly:** an earlier version of this
    /// measurement compared 60x `solve_d()` alone against 60x
    /// `quote_with_d()` and found the *new* pattern slower — because
    /// `quote_with_d` also runs its own `solve_new_output_reserve`
    /// Newton solve internally (genuinely, correctly — that part depends
    /// on the specific delta each time and can't be memoized), so that
    /// comparison wasn't measuring what the fix actually changed. This
    /// version compares the real old behavior (60x full `quote()` calls,
    /// each independently re-solving `D` via `solve_d` *and* running its
    /// own `solve_new_output_reserve`) against the real new behavior
    /// (1x `solve_d()` + 60x `quote_with_d()`, sharing that one `D`) —
    /// the actual before/after this session's fix produces.
    #[test]
    fn measure_before_after_call_pattern_timing() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let pool = StableSwap::new(
            vec![dec!(15_000_000), dec!(8_000_000), dec!(9_500_000)],
            dec!(100),
            dec!(0.997),
        )
        .unwrap();

        let start = Instant::now();
        for i in 0..60 {
            let _ = pool.quote(Decimal::from(i * 1000)).unwrap();
        }
        let before_style = start.elapsed();

        let start = Instant::now();
        let (d, _) = pool.solve_d().unwrap();
        for i in 0..60 {
            let _ = pool.quote_with_d(Decimal::from(i * 1000), d).unwrap();
        }
        let after_style = start.elapsed();

        println!("60x quote() (old: each re-solves D from scratch):        {before_style:?}");
        println!("1x solve_d() + 60x quote_with_d() (new: D shared):        {after_style:?}");
        println!(
            "speedup: {:.2}x",
            before_style.as_secs_f64() / after_style.as_secs_f64().max(0.000001)
        );
    }
}
