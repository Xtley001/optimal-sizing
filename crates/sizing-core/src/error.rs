//! `SizingError`. See `docs/API.md § error.rs` for the naming authority;
//! this file must not deviate from it.

/// The single error type returned by every fallible operation in
/// `sizing-core`.
#[derive(Debug, Clone, PartialEq)]
pub enum SizingError {
    /// A reserve, fee, or reference_price argument was zero, negative,
    /// or otherwise outside the domain the curve requires.
    InvalidReserves {
        /// Human-readable detail on which argument was invalid and why.
        detail: String,
    },

    /// StableSwap's Newton's-method solver exceeded MAX_NEWTON_ITERATIONS
    /// (255, matching Curve's own contract implementation) without
    /// converging to within NEWTON_TOLERANCE (1e-6 relative).
    DidNotConverge {
        /// Number of iterations actually attempted before giving up.
        iterations_attempted: u32,
    },

    /// max_size (or reserve-based capacity) fully excludes the profitable
    /// region — there is no valid trade size to return.
    NoProfitableSize,

    /// reference_price implies the pool is not mispriced in the profitable
    /// direction (marginal price at delta_in = 0 does not exceed
    /// reference_price after fees). Not an error condition in the sense of
    /// a bug — this is the correct, expected response when there is
    /// nothing to arbitrage — but modeled as an error variant rather than
    /// a silent zero so callers cannot mistake "no trade" for "computed
    /// a size of zero by accident."
    NoArbitrageOpportunity,
}
