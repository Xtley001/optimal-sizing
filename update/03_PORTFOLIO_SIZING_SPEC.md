# Specification 03: Cross-Pool Portfolio Capital Allocation (`sizing-portfolio`)

> **Document**: `update/03_PORTFOLIO_SIZING_SPEC.md`  
> **Author**: Xtley001  
> **Crate Target**: `crates/sizing-portfolio` (New Workspace Crate)  
> **Status**: Ready for Implementation  
> **Standards Compliance**: Claude Build Master Skill § 2 & § 3  

---

## 1. Domain & Boundary Definition

### What This Document Owns
- Creation of the new `sizing-portfolio` crate.
- Mathematical formulation and dual-variable solver for multi-pool capital allocation under a shared total capital budget $C_{\text{total}}$.
- Definition of `PortfolioOpportunity`, `PortfolioAllocation`, `PortfolioSizer`, and `PortfolioResult` types.
- Convergence guarantees, property tests, and accuracy benchmarks.

### What This Document Does NOT Own
- AMM quoting or single-curve sizing math (delegates to `sizing-core`).
- Multi-hop path chaining (delegates to `sizing-router`).
- On-chain flash loan execution or contract invocation.

---

## 2. Mathematical Formulation: Dual Water-Filling

### 2.1 The Constrained Multi-Opportunity Optimization Problem
Consider $M$ independent, simultaneous arbitrage opportunities across arbitrary AMM pools (e.g. Uniswap v2, Curve, Balancer, Velodrome). 

For each opportunity $i \in \{1, \dots, M\}$:
- Quoting function: $\text{quote}_i(\Delta x_i)$
- Reference price: $P_i$
- Marginal profit: $\Pi_i(\Delta x_i) = \text{quote}_i(\Delta x_i) - P_i \cdot \Delta x_i - c_i$
- Maximum capacity constraint: $\Delta x_i \le \text{cap}_i$

The trader has a fixed capital constraint $C_{\text{total}}$ (e.g. \$250,000 flash loan limit or wallet reserve).

$$\max_{\Delta x_1, \dots, \Delta x_M} \quad \sum_{i=1}^M \Pi_i(\Delta x_i)$$

subject to:
$$\sum_{i=1}^M \Delta x_i \le C_{\text{total}}, \quad 0 \le \Delta x_i \le \text{cap}_i \quad \forall i \in \{1, \dots, M\}$$

### 2.2 Karush-Kuhn-Tucker (KKT) Optimality Conditions
Because each profit function $\Pi_i(\Delta x_i)$ is concave, the optimization problem is convex.

The Lagrangian function is:

$$\mathcal{L}(\Delta x_1, \dots, \Delta x_M, \lambda) = \sum_{i=1}^M \Pi_i(\Delta x_i) - \lambda \cdot \left(\sum_{i=1}^M \Delta x_i - C_{\text{total}}\right)$$

At the global optimum, there exists a unique dual shadow price $\lambda^* \ge 0$ such that for each opportunity $i$:

$$\frac{\partial \mathcal{L}}{\partial \Delta x_i} = \Pi_i'(\Delta x_i) - \lambda^* = \text{quote}_i'(\Delta x_i) - P_i - \lambda^* = 0$$

$$\implies \text{quote}_i'(\Delta x_i) = P_i + \lambda^*$$

### 2.3 The Core Algorithmic Breakthrough
Solving for the optimal allocation under dual shadow price $\lambda$ on pool $i$ is **mathematically identical to single-pool sizing against an elevated reference price**:

$$P_{i,\text{effective}} = P_i + \lambda$$

Therefore, the portfolio optimizer:
1. Performs a 1-dimensional bisection over the shadow price $\lambda \in [0, \lambda_{\text{max}}]$.
2. For any candidate $\lambda$, each pool independently evaluates:
   $$\Delta x_i^*(\lambda) = \text{pool}_i.\text{optimal\_size}(P_i + \lambda, \text{constraints}_i)$$
3. Aggregates total capital consumed:
   $$X(\lambda) = \sum_{i=1}^M \Delta x_i^*(\lambda)$$
4. Because $X(\lambda)$ is strictly monotonically non-increasing in $\lambda$, bisection converges to $\lambda^*$ satisfying $X(\lambda^*) = C_{\text{total}}$ in at most **32 bisection steps** ($\mathcal{O}(\log(1/\epsilon))$).

---

## 3. Crate Architecture & Types

File path: `crates/sizing-portfolio/src/lib.rs`

### 3.1 Data Structures

```rust
use rust_decimal::Decimal;
use sizing_core::traits::SizingAlgorithm;
use sizing_core::types::{GuaranteeTier, SizingConstraints, SizingResult};
use sizing_core::error::SizingError;

/// An individual arbitrage opportunity candidate in a multi-pool portfolio.
pub struct PortfolioOpportunity {
    /// Human-readable identifier (e.g. "uniswap_v2_usdc_weth").
    pub id: String,
    /// Boxed AMM pool implementing SizingAlgorithm.
    pub pool: Box<dyn SizingAlgorithm + Send + Sync>,
    /// Base reference market price for this opportunity.
    pub reference_price: Decimal,
    /// Pool-specific execution constraints (gas cost, capacity cap).
    pub constraints: SizingConstraints,
}

/// Allocated capital and expected return for an opportunity in the solved portfolio.
#[derive(Debug, Clone)]
pub struct AllocatedPosition {
    /// Identifier of the opportunity.
    pub id: String,
    /// Allocated optimal capital size Δx_i*.
    pub allocated_size: Decimal,
    /// Expected net profit from this allocation.
    pub expected_profit: Decimal,
    /// Individual guarantee tier from the underlying pool.
    pub guarantee_tier: GuaranteeTier,
}

/// The global result of multi-pool portfolio optimization.
#[derive(Debug, Clone)]
pub struct PortfolioResult {
    /// Total capital deployed across all opportunities: sum(Δx_i*).
    pub total_capital_used: Decimal,
    /// Total expected profit net of all execution costs: sum(expected_profit_i).
    pub total_expected_profit: Decimal,
    /// Equilibrium shadow price λ* (marginal return per additional unit of capital).
    pub shadow_price: Decimal,
    /// List of per-opportunity allocations.
    pub allocations: Vec<AllocatedPosition>,
}
```

### 3.2 The `PortfolioSizer` Engine

```rust
/// Solves constrained multi-pool capital allocation across heterogeneous AMM curves.
pub struct PortfolioSizer {
    /// Total available capital budget (e.g. flash loan size).
    pub total_capital_budget: Decimal,
    /// Convergence tolerance for capital budget bisection (e.g. 1e-4).
    pub tolerance: Decimal,
}

impl PortfolioSizer {
    /// Constructs a new PortfolioSizer.
    pub fn new(total_capital_budget: Decimal) -> Result<Self, SizingError>;

    /// Optimizes capital allocation across the provided opportunities.
    pub fn optimize(
        &self,
        opportunities: &[PortfolioOpportunity],
    ) -> Result<PortfolioResult, SizingError>;
}
```

---

## 4. Acceptance Criteria & Test Contract

1. **Unconstrained Regime**:
   - When `total_capital_budget` exceeds the sum of unconstrained optima ($\sum \Delta x_i^*(0) < C_{\text{total}}$), verify that shadow price $\lambda^* = 0$ and every pool receives its exact unconstrained optimum.
2. **Constrained Water-Filling**:
   - When `total_capital_budget` is binding, verify that:
     1. Total allocated capital strictly satisfies $\sum \Delta x_i^* \le C_{\text{total}}$.
     2. Higher-margin pools receive capital before lower-margin pools.
     3. Marginal return $\Pi_i'(\Delta x_i^*)$ is equalized across all non-saturated pools.
3. **Property Test (`proptest`)**:
   - `prop_portfolio_allocation_monotonic_with_budget`: Allocations never decrease as budget increases.
   - `prop_portfolio_never_exceeds_budget`: Sum of allocations $\le C_{\text{total}} + \text{tolerance}$ holds for all draws.
4. **Execution Command**:
   ```bash
   cargo test -p sizing-portfolio
   ```
