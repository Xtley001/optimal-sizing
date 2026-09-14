//! Shared post-processing: given a curve's raw profit function, restricts
//! the search domain to delta_in values where gross profit exceeds
//! fixed_cost, and clips to max_size / available reserves. Applied
//! identically by all three curve implementations — do not duplicate
//! this logic inside cpmm.rs, stableswap.rs, or pmm.rs. See
//! `docs/API.md § profit.rs` and `docs/whitepaper.md § 5.4`.

use rust_decimal::Decimal;

use crate::error::SizingError;
use crate::types::SizingConstraints;

/// Shared fixed-cost/capacity domain restriction, applied identically by
/// all three curves — see `docs/whitepaper.md § 5.4`.
pub struct NetProfit;

impl NetProfit {
    /// Restricts `raw_domain` to `constraints.max_size` (capacity clipping).
    /// Returns `SizingError::NoProfitableSize` if the resulting domain is
    /// empty (e.g. `max_size <= raw_domain.0`).
    ///
    /// DECISION MADE (this project's stated decision-logging discipline, logged Session 5): this signature takes
    /// only `constraints` and a `(lo, hi)` interval — no profit function —
    /// so it cannot itself evaluate "where does gross profit exceed
    /// fixed_cost" (API.md's doc comment describes that in prose, but it
    /// isn't computable from this signature alone). whitepaper.md § 5.4
    /// confirms this reading: for Cpmm, "the library **separately**
    /// verifies the resulting Δx* still clears the c threshold before
    /// returning it" — i.e. each curve, not this function, performs the
    /// fixed_cost check after calling restrict_domain, exactly as
    /// implemented in cpmm.rs / stableswap.rs / pmm.rs's optimal_size.
    /// restrict_domain's real job is the max_size / capacity clip only.
    /// Needs adding to API.md.
    pub fn restrict_domain(
        constraints: &SizingConstraints,
        raw_domain: (Decimal, Decimal),
    ) -> Result<(Decimal, Decimal), SizingError> {
        let (lo, hi) = raw_domain;
        let hi = match constraints.max_size {
            Some(max_size) => hi.min(max_size),
            None => hi,
        };
        if hi <= lo {
            return Err(SizingError::NoProfitableSize);
        }
        Ok((lo, hi))
    }
}
