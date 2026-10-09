# Specification 01: Advanced AMM Curves (Velodrome Stable & DLMM)

> **Document**: `update/01_NEW_CURVES_SPEC.md`  
> **Author**: Xtley001  
> **Crate Target**: `sizing-core`  
> **Status**: Ready for Implementation  
> **Standards Compliance**: Claude Build Master Skill § 2 (Self-contained, exact names, clear boundaries, testable acceptance criteria)  

---

## 1. Domain & Boundary Definition

### What This Document Owns
- Mathematical specification, invariant formulation, and exact quote functions for:
  1. **Velodrome / Aerodrome Stable AMM Curve** ($x^3 y + x y^3 = k$).
  2. **Trader Joe / Meteora DLMM (Liquidity Book)** bin-based AMM.
- Rust implementations implementing `PricingCurve` and `SizingAlgorithm` traits.
- Exact Newton-Raphson update equations and convergence criteria.
- Unit tests, fixed regression cases, and property-based fuzzing tests (`proptest`).

### What This Document Does NOT Own
- Net profit subtraction and gas domain clipping (delegated to `sizing_core::profit::NetProfit`).
- Multi-hop composition (delegated to `sizing-router`).
- On-chain RPC reading or state fetching (caller-provided reserves).

---

## 2. Curve 1: Velodrome / Aerodrome Stable Curve ($x^3 y + x y^3 = k$)

The primary automated market maker curve powering Aerodrome on Base and Velodrome on Optimism. Designed for correlated or pegged assets (e.g. USDC/USDT, wstETH/ETH).

### 2.1 Mathematical Formulation

#### Invariant Equation
$$f(x, y) = x^3 y + x y^3 = k$$

Where $x > 0$ is the input asset reserve and $y > 0$ is the output asset reserve.

#### Solving for Post-Swap Output Reserve $y'$
Given input size $\Delta x \ge 0$ and fee retention $f \in (0, 1]$, the post-swap input reserve is:

$$x' = x + f \cdot \Delta x$$

We must solve for the post-swap output reserve $y'$ satisfying:

$$g(y') = x' \cdot (y')^3 + (x')^3 \cdot y' - k = 0$$

#### Newton-Raphson Iteration
Differentiating $g(y')$ with respect to $y'$:

$$g'(y') = 3 x' \cdot (y')^2 + (x')^3$$

Because $x' > 0$ and $y' \ge 0$:
- $g'(y') > 0$ strictly (function is strictly monotonically increasing).
- $g''(y') = 6 x' \cdot y' \ge 0$ (function is strictly convex on $y' \ge 0$).

Therefore, Newton's method is **quadratically and monotonically convergent** when initialized from $y_0 = y$:

$$y_{m+1} = y_m - \frac{x' \cdot y_m^3 + (x')^3 \cdot y_m - k}{3 x' \cdot y_m^2 + (x')^3}$$

#### Quoted Output
$$\text{quote}(\Delta x) = y - y'$$

#### Sizing and Optimality
Because the pool is convex in $y'$ as a function of $x'$, the trade profit function:

$$\Pi(\Delta x) = \text{quote}(\Delta x) - P \cdot \Delta x$$

is unimodal and strictly concave across $\Delta x \ge 0$. 

The optimal size $\Delta x^*$ is solved via bounded derivative bisection or Golden-Section search over $\Delta x \in [0, x_{\text{capacity}}]$, returning:

$$\text{GuaranteeTier::NumericallyGuaranteed \{ convergence\_conditions\_met: true \}}$$

### 2.2 Exact Rust Interface

File path: `crates/sizing-core/src/curves/velodrome_stable.rs`

```rust
use rust_decimal::Decimal;
use crate::error::SizingError;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// Velodrome/Aerodrome Stable AMM pool (x³y + xy³ = k).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VelodromeStable {
    /// Input asset reserve x. Must be strictly positive.
    pub reserve_x: Decimal,
    /// Output asset reserve y. Must be strictly positive.
    pub reserve_y: Decimal,
    /// Fee retention factor f (e.g., 0.9995 for a 5bps fee). In (0, 1].
    pub fee_retention: Decimal,
    /// Cached invariant k = x³y + xy³. Computed on construction.
    k: Decimal,
}

impl VelodromeStable {
    /// Constructs a new VelodromeStable curve.
    /// Returns SizingError::InvalidReserves if reserves <= 0 or fee_retention not in (0, 1].
    pub fn new(
        reserve_x: Decimal,
        reserve_y: Decimal,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError>;

    /// Invariant k = x³y + xy³.
    pub fn k(&self) -> Decimal;

    /// Solves for new output reserve y given new input reserve x_new using Newton's method.
    pub fn solve_y(&self, x_new: Decimal) -> Result<(Decimal, u32), SizingError>;
}

impl PricingCurve for VelodromeStable {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError>;
}

impl SizingAlgorithm for VelodromeStable {
    fn optimal_size(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError>;
}
```

---

## 3. Curve 2: Trader Joe / Meteora DLMM (Liquidity Book)

The discrete bin-based AMM model where liquidity is partitioned into localized bins of width $\text{bin\_step}$.

### 3.1 Mathematical Formulation

#### Discrete Bin Structure
Each bin $i \in \mathbb{Z}$ has a fixed price:

$$P_i = (1 + \text{bin\_step\_bps} / 10000)^i$$

Within bin $i$, tokens swap with **zero slippage** along a constant-sum curve:

$$P_i \cdot \Delta x_i + \Delta y_i = 0 \implies \Delta y_i = P_i \cdot f \cdot \Delta x_i$$

#### Multi-Bin Traversal Quoting
When input $\Delta x$ is traded:
1. Trade against the active bin $i$. If the bin has reserve $Y_i$, it can absorb at most $\Delta X_{i,\text{cap}} = \frac{Y_i}{P_i \cdot f}$.
2. If $\Delta x \le \Delta X_{i,\text{cap}}$, output is $\Delta y = P_i \cdot f \cdot \Delta x$.
3. If $\Delta x > \Delta X_{i,\text{cap}}$, exhaust bin $i$, decrement active bin to $i - 1$, and repeat on the residual input.

#### Sizing and Optimality
Because marginal price is a monotonically non-increasing step function (bin prices decrease as output asset is consumed), the profit function $\Pi(\Delta x)$ is **piecewise concave**.

The profit-maximizing input size $\Delta x^*$ terminates at the boundary of the lowest-priced bin where:

$$P_i \cdot f \ge P_{\text{reference}}$$

If the current bin already has $P_{\text{active}} \cdot f < P_{\text{reference}}$, return `SizingError::NoArbitrageOpportunity`.

Because the optimum is identified in exact discrete closed form:

$$\text{GuaranteeTier::ProvenOptimal}$$

### 3.2 Exact Rust Interface

File path: `crates/sizing-core/src/curves/dlmm.rs`

```rust
use rust_decimal::Decimal;
use crate::error::SizingError;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// An individual discrete liquidity bin in a DLMM pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlmmBin {
    /// Unique bin identifier.
    pub bin_id: i32,
    /// Price of the bin (output units per input unit).
    pub price: Decimal,
    /// Reserve of input token X available in this bin.
    pub reserve_x: Decimal,
    /// Reserve of output token Y available in this bin.
    pub reserve_y: Decimal,
}

/// Discretized Liquidity Book (DLMM) AMM curve.
#[derive(Debug, Clone)]
pub struct LiquidityBook {
    /// Active bin index in `bins`.
    pub active_bin_index: usize,
    /// Initialized bins, sorted in ascending order of bin_id.
    pub bins: Vec<DlmmBin>,
    /// Fee retention factor f (e.g., 0.998 for 20bps fee).
    pub fee_retention: Decimal,
}

impl LiquidityBook {
    /// Constructs a new DLMM curve from a list of bins and active bin index.
    pub fn new(
        bins: Vec<DlmmBin>,
        active_bin_index: usize,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError>;
}

impl PricingCurve for LiquidityBook {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError>;
}

impl SizingAlgorithm for LiquidityBook {
    fn optimal_size(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError>;
}
```

---

## 4. Module Integration Plan

1. **Register Modules**:
   - In `crates/sizing-core/src/curves/mod.rs`:
     ```rust
     pub mod velodrome_stable;
     pub mod dlmm;
     pub use velodrome_stable::VelodromeStable;
     pub use dlmm::{DlmmBin, LiquidityBook};
     ```
2. **Re-export in Core**:
   - In `crates/sizing-core/src/lib.rs`: export `VelodromeStable`, `DlmmBin`, and `LiquidityBook`.
3. **Register in Router**:
   - In `crates/sizing-router/src/route.rs`: add `Leg::VelodromeStable(VelodromeStable)` and `Leg::LiquidityBook(LiquidityBook)` to the `Leg` enum.

---

## 5. Acceptance Criteria & Test Contract

1. **Unit Tests**:
   - `velodrome_new_rejects_zero_or_negative_reserves`
   - `velodrome_quote_balanced_pool_matches_expected`
   - `velodrome_newton_converges_within_20_iterations`
   - `dlmm_quote_single_bin_zero_slippage`
   - `dlmm_quote_traverses_multiple_bins_correctly`
   - `dlmm_optimal_size_stops_at_unprofitable_bin_boundary`
2. **Property Tests (`proptest`)**:
   - `prop_velodrome_quote_strictly_monotonic`
   - `prop_velodrome_profit_concavity`
   - `prop_dlmm_quote_monotonic`
3. **Verification Command**:
   ```bash
   cargo test -p sizing-core --test velodrome_proptest --test dlmm_proptest
   ```
   Must pass with 0 errors.
