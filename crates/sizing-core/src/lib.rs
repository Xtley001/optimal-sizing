//! `sizing-core` — chain-agnostic profit-maximizing trade sizing against
//! CPMM, StableSwap, and PMM curves. See `docs/ARCHITECTURE.md` for module
//! boundaries and `docs/API.md` for the full public interface.
#![warn(missing_docs)]

/// The three concrete pricing curves — `Cpmm`, `StableSwap`, `Pmm` — each
/// implementing both `PricingCurve` and `SizingAlgorithm`.
pub mod curves;
/// `SizingError`, the single error type returned by every fallible
/// operation in this crate.
pub mod error;
/// `gas_aware_fixed_cost` — pure unit-conversion helper from gas price
/// to `fixed_cost` units, opt-in, no invented defaults (added —
/// `ROADMAP.md` "Gas-price-aware `fixed_cost`").
pub mod gas;
/// `NetProfit` — shared fixed-cost/capacity domain restriction, applied
/// identically by all three curves.
pub mod profit;
/// `SlippageBound` — worst-case `min_amount_out`, given an assumed
/// maximum staleness fraction. See module doc for the precision
/// tradeoff this makes (added — `ROADMAP.md` "Slippage / staleness
/// bound as a companion type").
pub mod slippage;
/// `PricingCurve` and `SizingAlgorithm` — the only two public traits.
pub mod traits;
/// `SizingConstraints`, `SizingResult`, `GuaranteeTier`.
pub mod types;

pub use error::SizingError;
pub use gas::gas_aware_fixed_cost;
pub use slippage::SlippageBound;
pub use traits::{PricingCurve, SizingAlgorithm};
pub use types::{GuaranteeTier, SizingConstraints, SizingResult};
