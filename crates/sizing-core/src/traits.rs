//! `PricingCurve`, `SizingAlgorithm` — the only two public traits in
//! `sizing-core`. See `docs/API.md § traits.rs` for the naming authority;
//! this file must not deviate from it.

use rust_decimal::Decimal;

use crate::error::SizingError;
use crate::types::{SizingConstraints, SizingResult};

/// A curve's raw pricing function: how much output a given input yields.
pub trait PricingCurve {
    /// Returns the output amount for a given input amount, post-fee.
    /// Returns `SizingError::InvalidReserves` if reserves are zero or negative.
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError>;
}

/// Computes the profit-maximizing trade size against an external
/// reference price. Every concrete curve (`Cpmm`, `StableSwap`, `Pmm`)
/// implements this alongside `PricingCurve`.
pub trait SizingAlgorithm: PricingCurve {
    /// Computes the profit-maximizing trade size against `reference_price`,
    /// subject to `constraints`. `reference_price` is denominated as
    /// (units of output asset) per (unit of input asset).
    fn optimal_size(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError>;
}
