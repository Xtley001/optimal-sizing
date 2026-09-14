# API Specification

This document is the naming authority for every public type, trait, function signature, and error variant in `sizing-core`. If code deviates from this document, the document wins unless it's updated first. Math derivations live in `whitepaper.md`; this document only specifies shapes and behavior.

## Crate: `sizing-core`

### `traits.rs`

```rust
pub trait PricingCurve {
    /// Returns the output amount for a given input amount, post-fee.
    /// Returns `SizingError::InvalidReserves` if reserves are zero or negative.
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError>;
}

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
```

Every concrete curve (`Cpmm`, `StableSwap`, `Pmm`) implements both traits. No other public traits exist in `sizing-core`.

### `types.rs`

```rust
pub struct SizingConstraints {
    /// Fixed execution cost, denominated in the output asset. Not optional —
    /// callers who want to ignore fixed costs pass `Decimal::ZERO` explicitly.
    /// There is no default; a missing value is a compile error, not an assumption.
    pub fixed_cost: Decimal,

    /// Upper bound on delta_in. `None` means unconstrained by the caller
    /// (still bounded internally by available reserves).
    pub max_size: Option<Decimal>,
}

pub struct SizingResult {
    pub optimal_delta: Decimal,
    pub expected_profit: Decimal,
    pub guarantee_tier: GuaranteeTier,
    /// `None` for closed-form results (Cpmm). `Some(n)` for iterative methods
    /// (StableSwap, Pmm), where n is the iteration count actually used.
    pub iterations: Option<u32>,
}

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
        /// True means no curvature-sign anomaly was found by the pre-search
        /// unimodality check (whitepaper.md § 5.3.2) across
        /// UNIMODALITY_CHECK_SAMPLES sample points of `quote()` itself —
        /// not of the profit function directly, since this check has no
        /// access to reference_price (see check_unimodality's own doc
        /// below for why that's the correct, price-independent thing to
        /// check). This is a sampled heuristic, not a proof: `true` should
        /// be read as "nothing was detected at this resolution," not
        /// "unimodality is confirmed" — a violation strictly between two
        /// adjacent samples will not be caught. False means an anomaly
        /// *was* detected; the result is still returned (a size is more
        /// useful than none) but the caller is on explicit notice that the
        /// search may have found a local rather than global optimum.
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
```

### `error.rs`

```rust
pub enum SizingError {
    /// A reserve, fee, or reference_price argument was zero, negative,
    /// or otherwise outside the domain the curve requires.
    InvalidReserves { detail: String },

    /// StableSwap's Newton's-method solver exceeded MAX_NEWTON_ITERATIONS
    /// (255, matching Curve's own contract implementation) without
    /// converging to within NEWTON_TOLERANCE (1e-6 relative).
    DidNotConverge { iterations_attempted: u32 },

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
```

### `curves/cpmm.rs`

```rust
pub struct Cpmm {
    pub x: Decimal,           // input-asset reserve
    pub y: Decimal,           // output-asset reserve
    pub fee_retention: Decimal, // e.g. dec!(0.997) for a 30bps fee
}

impl Cpmm {
    /// Constructs a new Cpmm. Returns SizingError::InvalidReserves if
    /// x <= 0, y <= 0, or fee_retention is not in (0, 1].
    pub fn new(x: Decimal, y: Decimal, fee_retention: Decimal) -> Result<Self, SizingError>;

    /// Validation-only helper: computes the optimal size via bisection
    /// search instead of the closed form, for use in the proptest suite
    /// to confirm the two methods agree. Not part of the public sizing
    /// path — SizingAlgorithm::optimal_size always uses the closed form.
    ///
    /// Stopping criterion (added — Session 2, previously unspecified):
    /// bisects until the bracket width is within NEWTON_TOLERANCE (1e-6
    /// relative, from curves::stableswap — reused rather than inventing a
    /// new constant) of the upper bracket bound.
    pub fn bisection_fallback(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError>;
}
```

Closed-form implementation detail (see `whitepaper.md § 5.1` for the derivation): `optimal_size` returns `SizingError::NoArbitrageOpportunity` when `fee_retention * y / x <= reference_price`, and otherwise computes `delta_in = (sqrt(x * y * fee_retention / reference_price) - x) / fee_retention`, then applies `profit::NetProfit` for `fixed_cost`/`max_size` before returning.

### `curves/concentrated_liquidity.rs` (added)

```rust
pub struct ConcentratedLiquidity {
    pub liquidity: Decimal,           // L
    pub sqrt_price_lower: Decimal,    // sqrt(price) at the lower tick bound
    pub sqrt_price_upper: Decimal,    // sqrt(price) at the upper tick bound
    pub sqrt_price_current: Decimal,  // sqrt(price) now — must be within [lower, upper]
    pub fee_retention: Decimal,       // same convention as Cpmm::fee_retention
}

impl ConcentratedLiquidity {
    /// Returns SizingError::InvalidReserves if liquidity <= 0, bounds
    /// aren't strictly ordered, sqrt_price_current is out of range, or
    /// fee_retention is not in (0, 1].
    pub fn new(
        liquidity: Decimal,
        sqrt_price_lower: Decimal,
        sqrt_price_upper: Decimal,
        sqrt_price_current: Decimal,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError>;
}
```

Single tick range only — see `docs/ARCHITECTURE.md § ConcentratedLiquidity` for why this still gets a closed form and `GuaranteeTier::ProvenOptimal` (it's `Cpmm`'s own math on Uniswap v3's "virtual reserves," with the tick boundary folded in as an extra `max_size`-style cap) and for the explicit multi-tick-crossing non-goal. `quote()` returns `SizingError::InvalidReserves` (not a silent clamp) for a trade that would cross the lower tick bound.

### `curves/stableswap.rs`

```rust
pub struct StableSwap {
    pub reserves: Vec<Decimal>, // length n, all > 0
    pub amplification: Decimal, // "A", >= 1
    pub fee_retention: Decimal,
}

pub const MAX_NEWTON_ITERATIONS: u32 = 255;
pub const NEWTON_TOLERANCE: Decimal = dec!(0.000001); // 1e-6, relative

impl StableSwap {
    pub fn new(reserves: Vec<Decimal>, amplification: Decimal, fee_retention: Decimal)
        -> Result<Self, SizingError>;

    /// Solves for the invariant D via Newton's method. Exposed publicly
    /// because callers building on top of this crate frequently need D
    /// independently of a sizing call (e.g. for LP share pricing).
    pub fn solve_d(&self) -> Result<(Decimal, u32), SizingError>; // (D, iterations)
}
```

Newton iteration formula (see `whitepaper.md § 5.2` for the full derivation and convergence argument — this is Curve's own published `get_D` algorithm, not a novel method, and the whitepaper cites it as such rather than claiming originality):

```
D_next = (A * n^n * S + D_P * n) * D / ((A * n^n - 1) * D + (n + 1) * D_P)
where S = sum(reserves), D_P = D^(n+1) / (n^n * product(reserves))
```

**Numerical implementation note (added — Session 3):** `D_P` is computed iteratively (`D_P = D; for each reserve: D_P = D_P * D / (n * reserve)`) rather than via the single `D^(n+1)` power shown above, to avoid overflowing `rust_decimal`'s ~28–29 digit range at realistic pool sizes. Same value, different computation path — see `whitepaper.md § 5.2` "Numerical note on `D_P`" for the full explanation.

**`PricingCurve::quote` / `SizingAlgorithm::optimal_size` (added — Session 3, previously unspecified beyond "see the implementation"):** the actual swap formula is Curve's `get_y` algorithm — see `whitepaper.md § 5.2.4` for the full iteration. Index convention: `reserves[0]` is the input asset, `reserves[1]` is the output asset, `reserves[2..]` are other pool assets held fixed. The sizing search brackets the root of a central-finite-difference estimate of `Profit'(delta_in)` (step size scaled by `NEWTON_TOLERANCE`) rather than using an analytic derivative — see `whitepaper.md § 5.2` "Sizing root-find, concretely."

### `curves/pmm.rs`

```rust
pub struct Pmm {
    pub base_reserve: Decimal,
    pub quote_reserve: Decimal,
    pub oracle_price: Decimal,   // "i" in WooFi's notation
    pub k: Decimal,              // curvature parameter, WooFi's published model
}

pub const GOLDEN_RATIO_INV: Decimal = dec!(0.6180339887); // (sqrt(5) - 1) / 2
pub const GOLDEN_SECTION_TOLERANCE: Decimal = dec!(0.000001);

/// Number of evenly spaced sample points check_unimodality evaluates across
/// the search interval before running golden-section search. Fixed for v1 —
/// not caller-configurable, not inferred from interval width. See
/// whitepaper.md § 5.3.2 for why this value exists and what its false-negative
/// characteristics are (a violation strictly between two adjacent samples is
/// not detected at any sample count).
pub const UNIMODALITY_CHECK_SAMPLES: u32 = 200;

impl Pmm {
    pub fn new(base_reserve: Decimal, quote_reserve: Decimal, oracle_price: Decimal, k: Decimal)
        -> Result<Self, SizingError>;

    /// Runs the pre-search unimodality check described in whitepaper.md
    /// § 5.3.2, sampling UNIMODALITY_CHECK_SAMPLES points.
    ///
    /// CORRECTED (Session 4): this checks quote()'s own curvature-sign
    /// consistency, not "the profit function" directly — the signature
    /// has no reference_price parameter, so it cannot compute profit. See
    /// whitepaper.md § 5.3.2 for why curvature-sign consistency is the
    /// correct, reference-price-independent thing to check instead.
    ///
    /// Returning `true` means no curvature-sign anomaly was detected at
    /// this sample resolution — it is not a proof that the profit function
    /// is unimodal for every possible reference_price. A violation
    /// confined entirely between two adjacent sample points will not be
    /// caught and this will still return `true`. See whitepaper.md §
    /// 5.3.2 "Known limitation" before treating `true` as a stronger claim
    /// than "nothing was detected."
    pub fn check_unimodality(&self, search_interval: (Decimal, Decimal)) -> bool;
}
```

**`PricingCurve::quote` / `SizingAlgorithm::optimal_size` (added — Session 4, previously entirely unspecified beyond the search method):** the actual WooFi pricing formula, sourced from WooFi's own dev docs, plus the piecewise reverse/basic regime model and index convention (`base_reserve` = input asset, `quote_reserve` = output asset, `r = 1`) that makes `check_unimodality` non-vacuous — see `whitepaper.md § 5.3.1` for the full formula and reasoning. Search interval upper bound: `min(1000 × base_reserve, 0.999 × pole)` where `pole = 1 / (k * oracle_price)` — see `whitepaper.md § 5.3`, "Search interval upper bound."

### `profit.rs`

```rust
/// Shared post-processing: given a curve's raw profit function, restricts
/// the search domain to delta_in values where gross profit exceeds
/// fixed_cost, and clips to max_size / available reserves. Applied
/// identically by all three curve implementations — do not duplicate
/// this logic inside cpmm.rs, stableswap.rs, or pmm.rs.
pub struct NetProfit;

impl NetProfit {
    pub fn restrict_domain(
        constraints: &SizingConstraints,
        raw_domain: (Decimal, Decimal),
    ) -> Result<(Decimal, Decimal), SizingError>; // returns SizingError::NoProfitableSize if empty
}
```

### `slippage.rs` (added — `ROADMAP.md` "Slippage / staleness bound as a companion type")

```rust
pub struct SlippageBound {
    pub min_amount_out: Decimal,
    pub staleness_fraction: Decimal,
}

impl SlippageBound {
    /// Returns SizingError::InvalidReserves if quoted_output <= 0 or
    /// staleness_fraction is not in [0, 1).
    pub fn from_quote(quoted_output: Decimal, staleness_fraction: Decimal) -> Result<Self, SizingError>;
}
```

Applies `staleness_fraction` directly to an already-quoted output (`min_amount_out = quoted_output * (1 - staleness_fraction)`) rather than re-solving the curve at hypothetically-moved reserves — see `slippage.rs`'s own module doc and `docs/ARCHITECTURE.md § SlippageBound / gas_aware_fixed_cost` for why, and for what a more precise version would need that this one doesn't attempt.

### `gas.rs` (added — `ROADMAP.md` "Gas-price-aware `fixed_cost`")

```rust
/// Pure unit conversion, not a calibrated estimator — gas_units_per_swap
/// is a required caller argument, never defaulted. Returns
/// SizingError::InvalidReserves if any argument is negative.
pub fn gas_aware_fixed_cost(
    gas_price_wei: Decimal,
    gas_units_per_swap: Decimal,
    native_token_price_in_output_units: Decimal,
) -> Result<Decimal, SizingError>;
```

Computes `gas_price_wei * gas_units_per_swap / 1e18 * native_token_price_in_output_units`. Does not modify `SizingConstraints.fixed_cost` itself — a caller computes a value with this and passes it in separately, same as before this existed.

## What is not in the public API

No async functions anywhere in `sizing-core`. No RPC types, no chain-specific types (no `Address`, no `U256`), no strategy/signal types. If any of these appear during implementation, stop and flag it before proceeding — it means either `API.md` is out of date or the implementation has drifted out of scope.
