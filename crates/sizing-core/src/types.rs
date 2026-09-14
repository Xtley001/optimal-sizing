//! `SizingConstraints`, `SizingResult`, `GuaranteeTier`. See
//! `docs/API.md § types.rs` for the naming authority; this file must not
//! deviate from it. Math backing these fields lives in `docs/whitepaper.md`.

use rust_decimal::Decimal;

/// Constraints applied to a sizing call — see `docs/whitepaper.md § 5.4`
/// for how `fixed_cost`/`max_size` restrict the search domain.
#[derive(Debug, Clone)]
pub struct SizingConstraints {
    /// Fixed execution cost, denominated in the output asset. Not optional —
    /// callers who want to ignore fixed costs pass `Decimal::ZERO` explicitly.
    /// There is no default; a missing value is a compile error, not an assumption.
    pub fixed_cost: Decimal,

    /// Upper bound on delta_in. `None` means unconstrained by the caller
    /// (still bounded internally by available reserves).
    pub max_size: Option<Decimal>,
}

/// The result of a sizing call — the trade size, its net expected profit,
/// and an explicit `GuaranteeTier` stating how strong a guarantee that
/// size actually carries.
#[derive(Debug, Clone, PartialEq)]
pub struct SizingResult {
    /// The profit-maximizing trade size found.
    pub optimal_delta: Decimal,
    /// Net expected profit at `optimal_delta`, after `fixed_cost`.
    pub expected_profit: Decimal,
    /// Which of the three guarantee tiers this result carries.
    pub guarantee_tier: GuaranteeTier,
    /// `None` for closed-form results (Cpmm). `Some(n)` for iterative methods
    /// (StableSwap, Pmm), where n is the iteration count actually used.
    pub iterations: Option<u32>,
}

/// States precisely which of three guarantee levels a `SizingResult`
/// carries — see `docs/whitepaper.md`'s abstract and § 5 for why these
/// three tiers exist and differ per curve family.
#[derive(Debug, Clone, PartialEq)]
pub enum GuaranteeTier {
    /// Cpmm only. The returned value is the exact global maximum —
    /// see whitepaper.md § 5.1 for the concavity proof.
    ProvenOptimal,

    /// StableSwap only.
    NumericallyGuaranteed {
        /// True if the regularity conditions from whitepaper.md § 5.2.3
        /// (A >= 1, all reserves > 0, initial guess within the basin of
        /// convergence) were confirmed to hold for this call. If false,
        /// `optimal_size` returns `Err(SizingError::DidNotConverge)`
        /// instead of a result carrying this tier — a `SizingResult` is
        /// never returned with `convergence_conditions_met: false`.
        convergence_conditions_met: bool,
    },

    /// Pmm only.
    EmpiricallyValidated {
        /// True means no curvature-sign anomaly was found by the
        /// pre-search unimodality check (whitepaper.md § 5.3.2) across
        /// UNIMODALITY_CHECK_SAMPLES sample points of `quote()` itself —
        /// not of the profit function directly, since this check has no
        /// access to reference_price. This is a sampled heuristic, not a
        /// proof: `true` should be read as "nothing was detected at this
        /// resolution," not "unimodality is confirmed" — a violation
        /// strictly between two adjacent samples will not be caught.
        /// False means an anomaly *was* detected; the result is still
        /// returned (a size is more useful than none) but the caller is
        /// on explicit notice that the search may have found a local
        /// rather than global optimum.
        unimodality_confirmed: bool,
    },

    /// `sizing-router` only — never constructed inside `sizing-core`
    /// itself (`sizing-core` has no notion of a route). Carries each
    /// leg's own tier for transparency, but is not itself a claim of
    /// `ProvenOptimal`-strength optimality for the composed route, even
    /// when every leg is individually `ProvenOptimal` — see
    /// `docs/ARCHITECTURE.md § sizing-router` for why concavity doesn't
    /// automatically survive composition. Treat like `EmpiricallyValidated`,
    /// not like `ProvenOptimal`, regardless of the tiers inside it.
    ComposedFrom(Vec<GuaranteeTier>),
}
