//! `SlippageBound`. Per `ROADMAP.md`'s "Slippage / staleness bound as a
//! companion type" entry.
//!
//! **Scope, stated up front (logged per this project's stated decision-logging discipline):** a
//! fully curve-aware bound would re-solve `quote()` at hypothetically
//! moved reserves (e.g. "what if `x`/`y` shift by up to 1% adversarially
//! before my tx confirms") — but that requires constructing a
//! "reserves moved by X%" variant of an arbitrary curve, which isn't
//! expressible generically over `PricingCurve` today (no curve exposes a
//! `with_moved_reserves`-style method, and adding one per curve family
//! is its own scoped piece of work, not attempted here). This version
//! instead applies `staleness_fraction` directly to the already-quoted
//! output — `min_amount_out = quoted_output * (1 - staleness_fraction)`
//! — a simpler, curve-agnostic worst-case bound, documented honestly as
//! an approximation of "the price could move against me by up to this
//! much," not a curve-exact re-derivation of what reserves moving by
//! `staleness_fraction` would actually do to the quote.

use rust_decimal::Decimal;

use crate::error::SizingError;

/// A worst-case `min_amount_out` bound for a trade, given an assumed
/// maximum adverse price movement before the transaction confirms.
pub struct SlippageBound {
    /// The floor output amount to pass on-chain as `min_amount_out` (or
    /// equivalent), below which the transaction should revert rather
    /// than execute.
    pub min_amount_out: Decimal,
    /// The `staleness_fraction` this bound was computed from, carried
    /// alongside the result so a caller can't lose track of which
    /// assumption produced it.
    pub staleness_fraction: Decimal,
}

impl SlippageBound {
    /// Computes a `min_amount_out` bound from an already-quoted output
    /// amount and an assumed maximum adverse reserve-movement fraction
    /// before the transaction confirms (e.g. `dec!(0.01)` for "up to 1%
    /// worse by the time this lands"). See module doc for the precision
    /// tradeoff this makes.
    ///
    /// Returns `SizingError::InvalidReserves` if `quoted_output <= 0` or
    /// `staleness_fraction` is not in `[0, 1)` (a fraction of 1 or more
    /// would floor the output at or below zero, which isn't a usable
    /// on-chain bound).
    pub fn from_quote(
        quoted_output: Decimal,
        staleness_fraction: Decimal,
    ) -> Result<Self, SizingError> {
        if quoted_output <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("quoted_output must be > 0, got {quoted_output}"),
            });
        }
        if staleness_fraction < Decimal::ZERO || staleness_fraction >= Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("staleness_fraction must be in [0, 1), got {staleness_fraction}"),
            });
        }
        let min_amount_out = quoted_output * (Decimal::ONE - staleness_fraction);
        Ok(Self {
            min_amount_out,
            staleness_fraction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn zero_staleness_returns_the_quote_unchanged() {
        let bound = SlippageBound::from_quote(dec!(1000), dec!(0)).unwrap();
        assert_eq!(bound.min_amount_out, dec!(1000));
    }

    #[test]
    fn ten_percent_staleness_floors_output_by_ten_percent() {
        let bound = SlippageBound::from_quote(dec!(1000), dec!(0.10)).unwrap();
        assert_eq!(bound.min_amount_out, dec!(900));
    }

    #[test]
    fn staleness_fraction_of_one_or_more_is_rejected() {
        assert!(matches!(
            SlippageBound::from_quote(dec!(1000), dec!(1.0)),
            Err(SizingError::InvalidReserves { .. })
        ));
        assert!(matches!(
            SlippageBound::from_quote(dec!(1000), dec!(1.5)),
            Err(SizingError::InvalidReserves { .. })
        ));
    }

    #[test]
    fn negative_quoted_output_is_rejected() {
        assert!(matches!(
            SlippageBound::from_quote(dec!(-1), dec!(0.01)),
            Err(SizingError::InvalidReserves { .. })
        ));
    }
}
