//! Golden-section search, unimodality pre-check. Struct/signatures per
//! `docs/API.md § curves/pmm.rs`. `whitepaper.md § 5.3` gives the search
//! method and the unimodality check but not the WooFi pricing formula
//! itself — that is sourced here from WooFi's own dev docs:
//! <https://learn.woo.org/woofi-docs/woofi-dev-docs/resources/the-math-behind-spmm>
//! ("The math behind sPMM"), cited inline below at each formula.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;

use crate::error::SizingError;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// A WooFi v2-style PMM ("proactive market maker") pool. Sizing against
/// this curve uses golden-section search, conditional on a runtime
/// unimodality check rather than an assumed guarantee — see
/// `whitepaper.md § 5.3`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pmm {
    /// Reserve of the base (input) asset.
    pub base_reserve: Decimal,
    /// Reserve of the quote (output) asset.
    pub quote_reserve: Decimal,
    /// Oracle price ("i" in WooFi's notation).
    pub oracle_price: Decimal,
    /// Curvature parameter, WooFi's published model.
    pub k: Decimal,
}

/// `(sqrt(5) - 1) / 2`, the golden-section search ratio. Full `Decimal`
/// precision (28 significant digits) — Session 9 fix: this was
/// previously hand-truncated to 10 digits (`dec!(0.6180339887)`) inside a
/// library that otherwise uses `Decimal` specifically to avoid
/// float-style truncation error; over `UNIMODALITY_CHECK_SAMPLES`-scale
/// iteration counts, that truncation was plausibly the dominant error
/// source in golden-section convergence, not `Decimal`'s own precision.
/// This value was computed via `Decimal`'s own `.sqrt()` at test time,
/// not transcribed from memory — see
/// `golden_ratio_inv_matches_runtime_sqrt_computation` below, which
/// verifies this literal against a fresh `(Decimal::from(5).sqrt() -
/// Decimal::ONE) / Decimal::TWO` computation on every test run, so a
/// future edit that drifts the literal will fail loudly instead of
/// silently reintroducing truncation error.
pub const GOLDEN_RATIO_INV: Decimal = dec!(0.6180339887498948482045868344);
/// Convergence tolerance for the golden-section search.
pub const GOLDEN_SECTION_TOLERANCE: Decimal = dec!(0.000001);

/// Number of evenly spaced sample points check_unimodality evaluates across
/// the search interval before running golden-section search. Fixed for v1 —
/// not caller-configurable, not inferred from interval width. See
/// whitepaper.md § 5.3.2 for why this value exists and what its false-negative
/// characteristics are (a violation strictly between two adjacent samples is
/// not detected at any sample count).
pub const UNIMODALITY_CHECK_SAMPLES: u32 = 200;

// DECISION MADE (this project's stated decision-logging discipline, logged Session 4): whitepaper.md § 5.3 names
// WooFi v2 as the pricing model but doesn't reproduce its formula. Sourced
// from WooFi's own dev docs (URL above, "The math behind sPMM"):
//
//   basic sell:   sellBase(ΔB)        = ΔB * p / (1 + k*ΔB*p)
//   reverse sell: reverseSellBase(ΔB) = ΔB * p / (1 - k*ΔB*p*r)
//
// where p = oracle_price (our struct has no separate spread field, so p is
// used directly rather than WooFi's mid-price/spread-derived ask price),
// and r ∈ [0,1] is WooFi's rebalance coefficient (r=1: full slippage paid
// to an arbitrageur rebalancing the pool; r=0: no rebalance discount).
//
// Our struct has no `r` field. This library's optimal_size caller *is* the
// rebalancing arbitrageur WooFi's docs describe ("another user wants to
// take the other side of the trade... the liquidity provider has to pay
// negative slippage to the arbitrageur to bring the balance of the pool
// back") — so r=1 is used whenever the trade is moving the pool toward
// balance. base_reserve/quote_reserve determine, via the implied balance
// point at the oracle price, how much of a given quote() call falls in the
// rebalancing (reverse, convex, r=1) regime versus the ordinary
// (basic, concave) regime once the pool crosses back through balance
// mid-trade. This whole mapping (index convention, r=1, the piecewise
// crossover) needs to land in API.md/whitepaper.md — it is the concrete
// design that makes UNIMODALITY_CHECK_SAMPLES/check_unimodality meaningful
// rather than vacuous, since a pure single-regime formula is provably
// always concave (see Session 4 handoff for the full curvature analysis).
impl Pmm {
    /// Constructs a new `Pmm`. Returns `SizingError::InvalidReserves` if
    /// `base_reserve`, `quote_reserve`, `oracle_price`, or `k` is `<= 0`.
    pub fn new(
        base_reserve: Decimal,
        quote_reserve: Decimal,
        oracle_price: Decimal,
        k: Decimal,
    ) -> Result<Self, SizingError> {
        if base_reserve <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("base_reserve must be > 0, got {base_reserve}"),
            });
        }
        if quote_reserve <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("quote_reserve must be > 0, got {quote_reserve}"),
            });
        }
        if oracle_price <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("oracle_price must be > 0, got {oracle_price}"),
            });
        }
        if k <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("k must be > 0, got {k}"),
            });
        }
        Ok(Self {
            base_reserve,
            quote_reserve,
            oracle_price,
            k,
        })
    }

    /// The base reserve that would exactly balance quote_reserve's value
    /// at the oracle price (base_reserve * oracle_price == quote_reserve).
    fn target_base_reserve(&self) -> Decimal {
        self.quote_reserve / self.oracle_price
    }

    /// The size of a base-selling trade (our quote()'s delta_in) that
    /// exactly brings the pool from its current state to balance. Zero if
    /// the pool is already at or past balance (no rebalancing regime
    /// applies to a base-selling trade in that case).
    fn crossover_delta(&self) -> Decimal {
        let target = self.target_base_reserve();
        if self.base_reserve < target {
            target - self.base_reserve
        } else {
            Decimal::ZERO
        }
    }

    /// reverseSellBase(ΔB) = ΔB * p / (1 - k*ΔB*p*r), r = 1. `None` at or
    /// beyond the pole (1 / (k*p)), where the model is no longer valid.
    fn reverse_sell_base(&self, delta: Decimal) -> Option<Decimal> {
        let denom = Decimal::ONE - self.k * delta * self.oracle_price;
        if denom <= Decimal::ZERO {
            return None;
        }
        Some(delta * self.oracle_price / denom)
    }

    /// sellBase(ΔB) = ΔB * p / (1 + k*ΔB*p).
    fn basic_sell_base(&self, delta: Decimal) -> Decimal {
        delta * self.oracle_price / (Decimal::ONE + self.k * delta * self.oracle_price)
    }

    /// Runs the pre-search unimodality check described in whitepaper.md
    /// § 5.3.2, sampling UNIMODALITY_CHECK_SAMPLES points. Called
    /// automatically by optimal_size; exposed publicly so callers can check
    /// before committing to a search on unusual parameter sets.
    ///
    /// DECISION MADE (this project's stated decision-logging discipline, logged Session 4): whitepaper.md § 5.3.2
    /// describes this as sampling "the profit function," but the public
    /// signature has no reference_price parameter and so cannot compute
    /// profit directly. What's checked instead is quote()'s own curvature
    /// (the sign of its discrete second derivative): if quote's curvature
    /// never changes sign across the interval **and that sign is never
    /// positive (i.e. quote is consistently concave, not convex)**,
    /// Profit(Δ) = quote(Δ) - P*Δ is unimodal-with-an-interior-maximum for
    /// *any* reference_price P, since subtracting a linear term preserves
    /// concavity/convexity exactly.
    ///
    /// **Session 9 correctness fix:** this previously checked only
    /// `sign_changes == 0` — that curvature never *flips* sign — without
    /// checking which sign it actually was. `reverse_sell_base(Δ) =
    /// Δ*p/(1-k*Δ*p)` (this file's own reverse-regime formula) has
    /// `f''(Δ) = 2*k*p²/(1-k*Δ*p)³ > 0` for `k, p > 0` in its valid
    /// domain — genuinely, provably **convex**, not concave — so a search
    /// interval confined entirely to the reverse regime (short of the
    /// basic-regime crossover) has curvature that never flips sign, but
    /// that sign is positive throughout, not negative. The old check
    /// reported `unimodality_confirmed: true` for exactly this case; a
    /// convex `Profit(Δ)` on a bounded interval has its maximum at an
    /// *endpoint*, not an interior stationary point, which golden-section
    /// search (as implemented below, always narrowing toward the larger
    /// of two interior sample points) has no correctness guarantee for.
    /// See `pmm_direct_tests.rs`'s
    /// `reverse_regime_only_interval_is_correctly_flagged_as_non_concave`
    /// for a concrete, analytically-derived reproduction, not just this
    /// comment's assertion. `docs/ARCHITECTURE.md § PMM unimodality fix`
    /// has the full writeup.
    ///
    /// Returning `true` means no curvature-sign anomaly (a flip, *or* a
    /// consistently-positive/convex sign) was found at this sample
    /// resolution — it is not a proof that the profit function is
    /// unimodal-with-an-interior-maximum for every possible reference_price.
    /// A violation confined entirely between two adjacent sample points
    /// will not be caught and this will still return `true`. See
    /// whitepaper.md § 5.3.2 "Known limitation" before treating `true` as
    /// a stronger claim than "nothing was detected."
    pub fn check_unimodality(&self, search_interval: (Decimal, Decimal)) -> bool {
        let (lo, hi) = search_interval;
        if hi <= lo {
            return true;
        }
        let width = hi - lo;
        let samples_dec = Decimal::from(UNIMODALITY_CHECK_SAMPLES);

        let mut quote_samples: Vec<Decimal> = Vec::new();
        for i in 0..=UNIMODALITY_CHECK_SAMPLES {
            let i_dec = Decimal::from(i);
            let delta = lo + width * (i_dec / samples_dec);
            if let Ok(q) = self.quote(delta) {
                quote_samples.push(q);
            }
            // quote() erroring (e.g. beyond the reverse-regime pole) simply
            // excludes that sample point from the sequence.
        }

        if quote_samples.len() < 3 {
            return true; // not enough points to assess curvature
        }

        // Discrete first differences (marginal value between consecutive samples).
        let marginal: Vec<Decimal> = quote_samples.windows(2).map(|w| w[1] - w[0]).collect();

        // Sign of the discrete second difference (curvature) between each
        // pair of consecutive marginal values. A single, unchanging,
        // non-positive sign throughout means quote is consistently concave
        // (or flat) — the actual condition golden-section maximization
        // needs. A consistently *positive* sign (convex) is now correctly
        // rejected too — see this method's doc comment for why treating
        // "never flips" alone as sufficient was the Session 9 bug.
        let mut prev_sign = 0i32;
        let mut sign_changes = 0u32;
        let mut saw_convex_sign = false;
        for w in marginal.windows(2) {
            let diff = w[1] - w[0];
            let sign = match diff.cmp(&Decimal::ZERO) {
                std::cmp::Ordering::Greater => 1,
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
            };
            if sign != 0 {
                if prev_sign != 0 && sign != prev_sign {
                    sign_changes += 1;
                }
                prev_sign = sign;
                if sign > 0 {
                    saw_convex_sign = true;
                }
            }
        }

        sign_changes == 0 && !saw_convex_sign
    }

    /// The outer bound `optimal_size`'s golden-section search runs
    /// against — `[0, search_upper_bound()]`. Exposed as its own method
    /// (Session 9) rather than inlined in `optimal_size`, specifically so
    /// tests can call the exact same computation `optimal_size` uses
    /// instead of independently re-deriving it — a test that duplicates
    /// this logic can silently drift from the real implementation (this
    /// codebase found exactly that happening to `pmm_proptest.rs`'s
    /// `prop_iteration_count_matches_formula`, which had been
    /// re-deriving the old, now-removed `base_reserve * 1000` formula).
    ///
    /// Derived from two real, curve-specific quantities: the pole (exact
    /// — `reverse_sell_base` is undefined beyond it) and `quote_reserve`
    /// (the curve's own hard capacity cap, via `quote()`'s
    /// `raw_output.min(self.quote_reserve)`), via the same
    /// doubling-bracket pattern already used elsewhere in this codebase
    /// (`Cpmm::bisection_fallback`, `StableSwap::optimal_size`,
    /// `sizing_router::Route::optimal_size`) — replacing what used to be
    /// a bare `base_reserve * 1000` with no derivation (its own comment
    /// admitted "no tighter bound is named in API.md/whitepaper.md," an
    /// an invented default that this project's decision-logging discipline exists to rule out).
    pub fn search_upper_bound(&self) -> Decimal {
        let pole = if self.k > Decimal::ZERO {
            Decimal::ONE / (self.k * self.oracle_price)
        } else {
            Decimal::MAX
        };
        // Stay strictly inside the pole with a safety margin.
        let pole_margin = pole * dec!(0.999);

        let mut upper = self.base_reserve.max(Decimal::ONE).min(pole_margin);
        let mut doubling_guard = 0u32;
        loop {
            let doubled = upper * Decimal::TWO;
            if doubled >= pole_margin {
                break; // can't expand further without crossing the pole margin
            }
            let output_at_doubled = match self.quote(doubled) {
                Ok(o) => o,
                Err(_) => break, // pole reached from the other regime's own domain limit
            };
            // Stop once we're within 0.01% of the pool's own hard
            // capacity cap (quote_reserve) — beyond that point there is
            // essentially no more real capacity to search over, so
            // continuing to double the bracket would only be searching
            // an ever-larger flat plateau.
            if output_at_doubled >= self.quote_reserve * dec!(0.9999) {
                upper = doubled;
                break;
            }
            upper = doubled;
            doubling_guard += 1;
            if doubling_guard > 200 {
                break;
            }
        }
        upper.min(pole_margin)
    }
}

impl PricingCurve for Pmm {
    /// Post-fee output for selling `delta_in` of the base asset for the
    /// quote asset. See the module-level DECISION MADE note for the
    /// piecewise reverse/basic regime split and the index convention.
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        if delta_in == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        let crossover = self.crossover_delta();
        let reverse_part = delta_in.min(crossover);
        let basic_part = delta_in - reverse_part;

        let reverse_output = if reverse_part > Decimal::ZERO {
            self.reverse_sell_base(reverse_part)
                .ok_or_else(|| SizingError::InvalidReserves {
                    detail: "delta_in reaches the reverse-regime pole".to_string(),
                })?
        } else {
            Decimal::ZERO
        };
        let basic_output = if basic_part > Decimal::ZERO {
            self.basic_sell_base(basic_part)
        } else {
            Decimal::ZERO
        };

        let raw_output = reverse_output + basic_output;
        // Cannot extract more than the pool's available quote reserve.
        Ok(raw_output.min(self.quote_reserve))
    }
}

impl SizingAlgorithm for Pmm {
    /// Golden-section search per whitepaper.md § 5.3, eq. (8) for the
    /// iteration count. Runs `check_unimodality` first per § 5.3.2;
    /// GuaranteeTier reflects the result rather than assuming it.
    ///
    /// fixed_cost/capacity handling per whitepaper.md § 5.4 (Session 5):
    /// same pattern as Cpmm/StableSwap — restrict_domain clips to
    /// max_size, and the fixed_cost threshold is checked separately here
    /// afterward.
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

        // Existence condition: marginal price at delta_in = 0 must exceed
        // reference_price. Marginal price at 0 is oracle_price (both
        // regimes' formulas reduce to p at delta=0).
        if self.oracle_price <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        // Search domain: [0, upper] — see search_upper_bound's own doc
        // comment (Session 9) for why this is derived from the pole and
        // quote_reserve rather than a fixed heuristic multiplier.
        let upper = self.search_upper_bound();

        let search_interval = (Decimal::ZERO, upper);
        let unimodality_confirmed = self.check_unimodality(search_interval);

        let profit_at = |delta: Decimal| -> Option<Decimal> {
            self.quote(delta).ok().map(|q| q - reference_price * delta)
        };

        // Golden-section search maximizing profit_at on [lo, hi], fixed
        // iteration count per eq. (8): n = ceil(ln(tol/L) / ln(GOLDEN_RATIO_INV)).
        let mut lo = Decimal::ZERO;
        let mut hi = upper;
        let l = hi - lo;

        let n_dec = (GOLDEN_SECTION_TOLERANCE / l).ln() / GOLDEN_RATIO_INV.ln();
        let iterations = n_dec.ceil().to_u32().unwrap_or(1).max(1);

        let gr = GOLDEN_RATIO_INV;
        let mut c = hi - gr * (hi - lo);
        let mut d = lo + gr * (hi - lo);

        for _ in 0..iterations {
            let f_c = profit_at(c).unwrap_or(Decimal::MIN);
            let f_d = profit_at(d).unwrap_or(Decimal::MIN);
            if f_c > f_d {
                hi = d;
            } else {
                lo = c;
            }
            c = hi - gr * (hi - lo);
            d = lo + gr * (hi - lo);
        }

        let unconstrained_delta = (lo + hi) / Decimal::TWO;

        let (_, optimal_delta) = crate::profit::NetProfit::restrict_domain(
            &constraints,
            (Decimal::ZERO, unconstrained_delta),
        )?;

        let output = self.quote(optimal_delta)?;
        let gross_profit = output - reference_price * optimal_delta;
        let expected_profit = gross_profit - constraints.fixed_cost;
        if expected_profit <= Decimal::ZERO {
            return Err(SizingError::NoProfitableSize);
        }

        Ok(SizingResult {
            optimal_delta,
            expected_profit,
            guarantee_tier: GuaranteeTier::EmpiricallyValidated {
                unimodality_confirmed,
            },
            iterations: Some(iterations),
        })
    }
}
