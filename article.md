# Optimal Trade Sizing for AMMs: Why Your Bot Is Bleeding Alpha (And the Math to Fix It)

*A definitive guide to closed-form solutions, Newton’s method, and tiered mathematical guarantees across Uniswap, Curve, Balancer, and WooFi in Rust and Python.*

---

**Author**: Xtley001 / `optimal-sizing` Core Team  
**Reading Time**: ~18 min read  
**Target Audience**: Quant Researchers, MEV Searchers, DeFi Protocol Engineers, Algorithmic Traders  
**Primary Keywords**: AMM trade sizing, Uniswap arbitrage formula, Curve StableSwap optimal size, MEV bot trade sizing, closed-form AMM sizing, DEX multi-hop routing, DeFi algorithmic trading, Rust DeFi library, `optimal-sizing`  
**GitHub Repository**: [https://github.com/Xtley001/optimal-sizing](https://github.com/Xtley001/optimal-sizing)  
**Crates.io**: `sizing-core`, `sizing-router`, `sizing-py`, `sizing-cli`  

---

## Executive Summary (TL;DR)

1. **The Dirty Secret of Production Bots**: Over 80% of DEX arbitrage and routing bots rely on naive grid searches (`for delta in range(...)`) or unverified bisection loops. At typical grid resolutions ($N = 100$), trade sizing misses the true optimum by **0.74%**, bleeding high-value alpha and frequently turning marginally profitable trades into reverted, gas-wasting failures.
2. **The Speed & Accuracy Penalty**: To achieve an accuracy within $10^{-4}$ (0.01%), a grid search requires $N = 10,000$ evaluations, consuming **10 to 160 milliseconds** per pool. In competitive MEV block auctions, this latency is an instant loss.
3. **The Solution (`optimal-sizing`)**: An open-source, chain-agnostic Rust engine (with native Python bindings via PyO3) that replaces ad-hoc loops with rigorous mathematical solvers:
   - **Constant-Product AMMs (Uniswap v2, Sushiswap)**: Solved in **closed form** in **45 µs** (220x faster than grid search), provably global optimum ($\mathcal{O}(1)$).
   - **Curve StableSwap (3pool & forks)**: Solved via **Newton-Raphson iteration** with quadratic convergence in **879 µs** (177x faster), with invariant memoization and formal regularity checks.
   - **Concentrated Liquidity (Uniswap v3)**: Closed form on virtual reserves bounded by tick ranges in **45 µs** (240x faster).
   - **Balancer Weighted Pools**: Closed-form power-law solver in **75 µs** (240x faster).
   - **Proactive Market Makers (WooFi v2, DODO)**: Golden-section search backed by runtime discrete second-derivative unimodality validation in **298 µs**.
4. **Explicit Guarantee Tiers**: Every calculation returns an explicit enum (`ProvenOptimal`, `NumericallyGuaranteed`, or `EmpiricallyValidated`), ensuring algorithms never confuse heuristic approximations with mathematical proofs.
5. **Zero Float Drift**: Built entirely on arbitrary-precision fixed-point math (`rust_decimal::Decimal`), eliminating floating-point rounding errors and catastrophic cancellation near liquidity poles.

---

## Table of Contents

- [1. Introduction: The Silent Tax on DeFi Arbitrage](#1-introduction-the-silent-tax-on-defi-arbitrage)
- [2. The Formal Optimization Problem](#2-the-formal-optimization-problem)
- [3. The Guarantee Tier Paradigm](#3-the-guarantee-tier-paradigm)
- [4. Mathematical Derivations Across 7 Curve Families](#4-mathematical-derivations-across-7-curve-families)
  - [4.1 Constant-Product AMMs (Uniswap v2) — Closed-Form Sizing](#41-constant-product-amms-uniswap-v2--closed-form-sizing)
  - [4.2 Curve StableSwap — Newton's Method & Invariant Memoization](#42-curve-stableswap--newtons-method--invariant-memoization)
  - [4.3 Proactive Market Makers (WooFi v2 & DODO) — Golden-Section & Unimodality](#43-proactive-market-makers-woofi-v2--dodo--golden-section--unimodality)
  - [4.4 Concentrated Liquidity (Uniswap v3) — Virtual Reserves & Tick Bounds](#44-concentrated-liquidity-uniswap-v3--virtual-reserves--tick-bounds)
  - [4.5 Balancer Weighted Pools — Power-Law Closed Form](#45-balancer-weighted-pools--power-law-closed-form)
  - [4.6 Curve CryptoSwap (Curve v2) — Dynamic Invariant Root-Finding](#46-curve-cryptoswap-curve-v2--dynamic-invariant-root-finding)
- [5. Empirical Benchmarks: Mathematical Solvers vs. Grid Search](#5-empirical-benchmarks-mathematical-solvers-vs-grid-search)
- [6. Battle-Hardened Architecture & Numeric Precision](#6-battle-hardened-architecture--numeric-precision)
  - [6.1 The Float Trap: Why IEEE-754 Destroys MEV](#61-the-float-trap-why-ieee-754-destroys-mev)
  - [6.2 Fixed Execution Costs & Gas Domain Restrictions](#62-fixed-execution-costs--gas-domain-restrictions)
  - [6.3 Derivative Step Sizing and Catastrophic Cancellation](#63-derivative-step-sizing-and-catastrophic-cancellation)
- [7. Multi-Hop Heterogeneous Routing (`sizing-router`)](#7-multi-hop-heterogeneous-routing-sizing-router)
- [8. Practical Implementation Guide](#8-practical-implementation-guide)
  - [8.1 Rust Implementation (`sizing-core`)](#81-rust-implementation-sizing-core)
  - [8.2 Python Implementation (`sizing-py`)](#82-python-implementation-sizing-py)
  - [8.3 Live Ethereum Mainnet Integration (`sizing-integration`)](#83-live-ethereum-mainnet-integration-sizing-integration)
  - [8.4 Command-Line Sizing (`sizing-cli`)](#84-command-line-sizing-sizing-cli)
- [9. Frequently Asked Questions (FAQ)](#9-frequently-asked-questions-faq)
- [10. Conclusion & Getting Started](#10-conclusion--getting-started)

---

## 1. Introduction: The Silent Tax on DeFi Arbitrage

In the high-stakes arena of decentralized finance (DeFi) and Maximal Extractable Value (MEV), execution algorithms obsess over microsecond RPC latency, builder priority fees, and private mempool relays (Flashbots Protect, bloXroute, Eden). Yet, an alarming number of trading desks, arbitrage bots, and smart order routers bleed substantial profit through an elementary flaw: **how they calculate their trade size**.

Consider a textbook triangular or spatial arbitrage opportunity. An automated market maker (AMM) pool is displaced from an external reference price (e.g., Binance, Coinbase, or an off-chain benchmark). 

If you trade **too little**, you leave free money on the table for the next searcher in the block.  
If you trade **too much**, slippage inside the pool eats your margin alive, and execution costs cause the transaction to yield zero or negative profit.

```
       Net Profit
           ^
           |             Optimal Peak (Δx*)
           |                    ▲
           |                   / \
           |                  /   \
           |    Sub-optimal  /     \  Over-sized (Slippage eats margin)
           |    (Under-sized)       \
           |    /                     \
  Zero ----+---/-----------------------\------------> Input Size (Δx)
           |  /                         \
  Fixed ---+-/ (Gas cost)                \ (Net negative)
  Cost     |/
```

How do most open-source bots solve for this peak? They run a **grid search** or a **blind bisection loop**:
```python
# The standard "production" bot anti-pattern:
best_profit = 0
best_size = 0
for step in range(1, 100):
    size = min_size + step * (max_size - min_size) / 100
    profit = quote(size) - reference_price * size - gas_cost
    if profit > best_profit:
        best_profit = profit
        best_size = size
```

This approach has two lethal defects:
1. **The Precision Deficit**: A 100-step grid introduces a **~0.74% error** on your sizing. In thin-margin arbitrage (where margins are often 5 to 20 basis points), a 0.74% sizing inaccuracy can wipe out 30% to 100% of your expected alpha.
2. **The Latency Trap**: Evaluating complex invariants (such as Curve's StableSwap or WooFi's sPMM) 100 to 1,000 times sequentially takes **tens to hundreds of milliseconds**. By the time your Python loop completes, the block is already sealed.

`optimal-sizing` was built to replace ad-hoc heuristics with **closed-form mathematics** and **provably convergent numerical algorithms**.

---

## 2. The Formal Optimization Problem

Across any automated market maker pool, given:
- Current input reserve $x$ and output reserve $y$
- Fee retention factor $f \in (0, 1]$ (e.g., $f = 0.997$ for a 30 bps fee)
- External reference price $P$ (denominated as output asset units per input asset unit)
- Fixed execution cost $C_{\text{gas}}$ (gas and protocol overhead, denominated in the output asset)

The arbitrageur's objective is to determine the trade size $\Delta x \ge 0$ that maximizes net post-fee, post-gas profit:

$$\max_{\Delta x \ge 0} \quad \Pi_{\text{net}}(\Delta x) = \text{quote}(\Delta x) - P \cdot \Delta x - C_{\text{gas}}$$

subject to:
$$\Delta x \le \Delta x_{\text{max}} \quad \text{and} \quad \text{quote}(\Delta x) \le y$$

Where $\text{quote}(\Delta x)$ is the pool's deterministic pricing function returning post-fee output tokens for input $\Delta x$.

The fundamental difficulty in DeFi is that **no two AMM curves share the same $\text{quote}(\Delta x)$ function**:
- **Uniswap v2**: $x \cdot y = k$ (Hyperbolic invariant)
- **Curve v1**: $A \cdot n^n \sum x_i + D = A \cdot D \cdot n^n + \frac{D^{n+1}}{n^n \prod x_i}$ (Hybrid sum-product invariant)
- **WooFi v2**: Piecewise rational functions switching between LP rebalancing incentives and concave quoting
- **Balancer**: $\prod x_i^{w_i} = k$ (Geometric mean invariant)
- **Uniswap v3**: Discontinuous piecewise liquidity distributions across localized price ticks

Treating all of these curves with a generic numerical black box is either computationally prohibitive or mathematically unsound.

---

## 3. The Guarantee Tier Paradigm

A core innovation of `optimal-sizing` is the **Guarantee Tier System**. In financial engineering, calling an optimizer shouldn't be a leap of faith. The library categorizes every result into an explicit type-level guarantee:

```rust
pub enum GuaranteeTier {
    /// Solved via analytical closed-form. The returned trade size is 
    /// mathematically proven to be the global unconstrained maximum.
    ProvenOptimal,

    /// Solved via Newton-Raphson iteration. Quadratic convergence is 
    /// guaranteed because pool regularity conditions (A >= 1, reserves > 0, 
    /// initial guess in contraction basin) were formally verified at runtime.
    NumericallyGuaranteed {
        convergence_conditions_met: bool,
    },

    /// Solved via golden-section search. Guaranteed optimal only if 
    /// the profit curve is unimodal. Unimodality is verified via 
    /// a runtime discrete curvature check.
    EmpiricallyValidated {
        unimodality_confirmed: bool,
    },

    /// Composed multi-hop route. Carries underlying tiers for transparency, 
    /// but joint concavity is not preserved under composition.
    ComposedFrom(Vec<GuaranteeTier>),
}
```

This guarantees that downstream systems can inspect the risk profile of a trade size before submitting capital to the mempool:

```
┌────────────────────────────────────────────────────────┐
│                   optimal-sizing                       │
├──────────────────┬─────────────────┬───────────────────┤
│   Curve Type     │     Method      │  Guarantee Tier   │
├──────────────────┼─────────────────┼───────────────────┤
│ Uniswap v2 / CPMM│ Closed-Form     │ ProvenOptimal     │
│ Uniswap v3 (CL)  │ Virtual CPMM    │ ProvenOptimal     │
│ Balancer V1/V2   │ Power-Law Form  │ ProvenOptimal     │
│ Curve StableSwap │ Newton-Raphson  │ NumericallyGuar.  │
│ Curve CryptoSwap │ Dynamic Newton  │ NumericallyGuar.  │
│ WooFi v2 (sPMM)  │ Golden-Section  │ EmpiricallyValid. │
│ DODO PMM         │ Golden-Section  │ EmpiricallyValid. │
│ Multi-Hop Route  │ Joint Golden    │ ComposedFrom      │
└──────────────────┴─────────────────┴───────────────────┘
```

---

## 4. Mathematical Derivations Across 7 Curve Families

Let us examine the exact mathematical machinery powering each curve family in `sizing-core`.

---

### 4.1 Constant-Product AMMs (Uniswap v2) — Closed-Form Sizing

For a pool with input reserve $x$, output reserve $y$, and fee retention factor $f = 1 - \text{fee}$ (e.g. $0.997$):

#### 1. The Quote Function
The standard constant-product formula $(x + f \cdot \Delta x)(y - \Delta y) = x \cdot y$ yields:

$$\text{quote}(\Delta x) = \frac{y \cdot f \cdot \Delta x}{x + f \cdot \Delta x}$$

#### 2. The Profit Function
$$\Pi(\Delta x) = \frac{y \cdot f \cdot \Delta x}{x + f \cdot \Delta x} - P \cdot \Delta x$$

#### 3. Concavity Proof
Differentiating with respect to $\Delta x$:

$$\Pi'(\Delta x) = \frac{y \cdot f \cdot x}{(x + f \cdot \Delta x)^2} - P$$

Differentiating a second time:

$$\Pi''(\Delta x) = -\frac{2 \cdot y \cdot f^2 \cdot x}{(x + f \cdot \Delta x)^3}$$

Because reserves $x > 0$, $y > 0$, and fee factor $f > 0$, for all physical trade sizes $\Delta x \ge 0$:

$$\Pi''(\Delta x) < 0 \quad \forall \Delta x \ge 0$$

**Conclusion**: The profit function $\Pi(\Delta x)$ is **strictly concave** across its entire domain. Any critical point where $\Pi'(\Delta x) = 0$ is not merely a local maximum—it is the **unique, global maximum**.

#### 4. Analytical Solution
Setting $\Pi'(\Delta x) = 0$:

$$\frac{y \cdot f \cdot x}{(x + f \cdot \Delta x)^2} = P \implies (x + f \cdot \Delta x)^2 = \frac{x \cdot y \cdot f}{P}$$

$$x + f \cdot \Delta x = \sqrt{\frac{x \cdot y \cdot f}{P}}$$

$$\Delta x^* = \frac{\sqrt{\frac{x \cdot y \cdot f}{P}} - x}{f}$$

#### 5. Arbitrage Existence Condition
A profitable opportunity exists if and only if the pool's marginal exchange rate at zero size exceeds the market reference price $P$:

$$\Pi'(0) > 0 \iff \frac{y \cdot f}{x} > P$$

If $\frac{y \cdot f}{x} \le P$, `sizing-core` immediately returns `SizingError::NoArbitrageOpportunity` in $\mathcal{O}(1)$ without executing square roots.

#### Worked Concrete Example
- $x = 1,200,000$ USDC
- $y = 800,000$ WETH
- $f = 0.997$ (30 bps fee)
- External Reference Price $P = 0.60$ USDC per WETH

Checking marginal zero-size price:
$$\frac{800,000 \cdot 0.997}{1,200,000} \approx 0.66467 > 0.60 \quad (\text{Opportunity exists})$$

Calculating radicand:
$$\text{radicand} = \frac{1,200,000 \cdot 800,000 \cdot 0.997}{0.60} = 1,595,200,000,000$$
$$\sqrt{\text{radicand}} \approx 1,263,012.2723077555$$
$$\Delta x^* = \frac{1,263,012.2723077555 - 1,200,000}{0.997} = \mathbf{63,201.87794158\dots} \text{ USDC}$$

Execution time in Rust: **45 microseconds**.

---

### 4.2 Curve StableSwap — Newton's Method & Invariant Memoization

Curve's StableSwap invariant blends constant-sum (zero slippage for pegged assets) and constant-product (infinite liquidity depth):

$$A \cdot n^n \sum_{i=1}^n x_i + D = A \cdot D \cdot n^n + \frac{D^{n+1}}{n^n \prod_{i=1}^n x_i}$$

Where $A$ is the amplification coefficient, $n$ is the number of tokens, and $D$ is the total invariant.

#### 1. Solving for the Invariant $D$
No closed-form solution exists for $D$ when $n \ge 3$. We solve it numerically via Newton's method:

$$D_{k+1} = \frac{\left(A \cdot n^n \cdot S + D_P \cdot n\right) \cdot D_k}{(A \cdot n^n - 1) \cdot D_k + (n + 1) \cdot D_P}$$

where $S = \sum x_i$ and $D_P = \frac{D^n}{n^n \prod x_i} \cdot D$.

To prevent numerical overflow in 28-digit fixed-point math, `sizing-core` evaluates $D_P$ iteratively:
```rust
let mut d_p = d;
for reserve in reserves {
    d_p = d_p * d / (Decimal::from(n) * reserve);
}
```

#### 2. Determining Output Reserve $y$ (`get_y`)
Given a candidate input size $\Delta x$, the new input reserve is $x_0' = x_0 + f \cdot \Delta x$. The new output reserve $y = x_1'$ is found by solving the invariant equation for $y$, yielding the Newton update:

$$y_{k+1} = \frac{y_k^2 + c}{2y_k + b - D}$$

where constants $c$ and $b$ are derived from the remaining fixed reserves and amplification $A$.

#### 3. The 1.5x Performance Breakthrough: Memoizing Invariant $D$
A standard sizing search makes 40 to 80 quote evaluations to find the optimal $\Delta x^*$. 
In naive implementations, each `quote()` recomputes $D$ from scratch via 5 to 15 Newton iterations.

`optimal-sizing` memoizes $D$ once per `optimal_size()` invocation:
```rust
// In curves/stableswap.rs:
let memoized_d = self.solve_d()?; // Solved once!

// Fast inner quote reusing memoized D:
let quote_fn = |delta: Decimal| -> Result<Decimal, SizingError> {
    self.quote_with_d(delta, memoized_d)
};
```
This single architectural optimization reduces StableSwap sizing latency from **1,400 µs to 879 µs** without risking state staleness.

---

### 4.3 Proactive Market Makers (WooFi v2 & DODO) — Golden-Section & Unimodality

WooFi v2 uses an oracle price $p$ and curvature parameter $k$ to synthesize order-book depth on-chain. Crucially, the quoting function is **piecewise asymmetric**:

1. **Standard Sell (Moving pool away from balance)**:
   $$\text{sellBase}(\Delta B) = \frac{\Delta B \cdot p}{1 + k \cdot \Delta B \cdot p} \quad (\text{Strictly concave, } \Pi'' < 0)$$
2. **Reverse Sell (Moving pool toward balance)**:
   $$\text{reverseSellBase}(\Delta B) = \frac{\Delta B \cdot p}{1 - k \cdot \Delta B \cdot p \cdot r} \quad (\text{Strictly convex!})$$

```
  Price / Marginal Return
      ^
      |         Convex Regime (Reverse Sell)        Concave Regime (Standard Sell)
      |          f''(Δ) > 0 (Incentivized)           f''(Δ) < 0 (Slippage dominates)
      |                    \                               /
      |                     \                             /
      |                      \                           /
      |                       ▼                         ▼
      |                        \                       /
      |                         -----------------------
      +-----------------------------------|----------------------------> ΔB (Input)
                                   Pool Balance Point
```

#### The Algorithmic Pitfall
Because $\text{reverseSellBase}$ has a positive second derivative ($\Pi'' > 0$), a naive optimization algorithm can easily get trapped or misidentify a local edge as a global optimum!

#### The Runtime Unimodality Safeguard
Before initiating golden-section search, `sizing-core` executes `check_unimodality`:
1. Samples the quoting function at $N = 32$ discrete points across the search bracket.
2. Computes the discrete first differences: $\Delta Q_i = \text{quote}(x_{i+1}) - \text{quote}(x_i)$.
3. Computes discrete second differences: $\Delta^2 Q_i = \Delta Q_{i+1} - \Delta Q_i$.
4. **Verifies concavity**: Requires $\Delta^2 Q_i \le 0$ across the active trade interval.

If concavity holds, golden-section search converges to the true optimum with guaranteed tolerance:

$$N_{\text{iter}} = \left\lceil \frac{\ln(\text{tol} / L)}{\ln(\phi^{-1})} \right\rceil, \quad \phi^{-1} = \frac{\sqrt{5} - 1}{2} \approx 0.6180339887$$

If a curvature anomaly is detected, the engine still returns the best-found candidate but flags `GuaranteeTier::EmpiricallyValidated { unimodality_confirmed: false }`, warning downstream routing logic.

---

### 4.4 Concentrated Liquidity (Uniswap v3) — Virtual Reserves & Tick Bounds

A Uniswap v3 position concentrates liquidity $L$ within a price interval $[p_a, p_b]$. Inside a single tick range, the curve behaves identically to a constant-product pool over **virtual reserves**:

$$x_{\text{virt}} = \frac{L}{\sqrt{P_{\text{curr}}}}, \quad y_{\text{virt}} = L \cdot \sqrt{P_{\text{curr}}}$$

Where $P_{\text{curr}}$ is the active pool price.

`optimal-sizing` computes the unconstrained optimal trade size $\Delta x^*$ using the closed-form CPMM equation directly on $(x_{\text{virt}}, y_{\text{virt}})$. It then clamps the solution against the tick boundary capacity:

$$\Delta x_{\text{tick\_limit}} = L \cdot \left(\frac{1}{\sqrt{P_{\text{target}}}} - \frac{1}{\sqrt{P_{\text{curr}}}}\right)$$

$$\Delta x_{\text{optimal}} = \min(\Delta x^*, \Delta x_{\text{tick\_limit}})$$

This provides **closed-form, provably optimal sizing in 45 µs** without stepping through tick arrays.

---

### 4.5 Balancer Weighted Pools — Power-Law Closed Form

Balancer pools generalize constant-product to $n$ assets with arbitrary normalized weights $w_i \in (0, 1)$ such that $\sum w_i = 1$:

$$\prod_{i=1}^n x_i^{w_i} = k$$

For a swap from asset $i$ (weight $w_i$) to asset $o$ (weight $w_o$) with fee retention $f$:

$$\text{quote}(\Delta x_i) = y_o \cdot \left(1 - \left(\frac{x_i}{x_i + f \cdot \Delta x_i}\right)^{w_i / w_o}\right)$$

Let $\alpha = w_i / w_o$. Setting the marginal derivative of profit equal to reference price $P$:

$$\Pi'(\Delta x_i) = y_o \cdot \alpha \cdot f \cdot x_i^\alpha \cdot (x_i + f \cdot \Delta x_i)^{-(\alpha + 1)} - P = 0$$

Solving analytically for $\Delta x_i^*$:

$$\Delta x_i^* = \frac{\left(\frac{y_o \cdot \alpha \cdot f \cdot x_i^\alpha}{P}\right)^{\frac{1}{\alpha + 1}} - x_i}{f}$$

`sizing-core` implements this analytical power-law solution using high-precision decimal exponentiation, achieving `GuaranteeTier::ProvenOptimal` in **75 µs**.

---

### 4.6 Curve CryptoSwap (Curve v2) — Dynamic Invariant Root-Finding

Curve v2 pools (such as the Tricrypto pool: USDT/WBTC/WETH) support unpegged, volatile assets. The invariant dynamically shifts between StableSwap and Constant-Product depending on pool imbalance:

$$K \cdot D^{n-1} \sum x_i + \prod x_i = \left(\frac{D}{n}\right)^n \cdot \gamma + K \cdot D^n$$

Where $K$ is a dynamic amplification variable keyed to distance from the internal price peg.

`sizing-core` uses a dynamic invariant solver that iteratively updates $K$ and $D$, converging to the optimal trade size in **1,202 µs** carrying `GuaranteeTier::NumericallyGuaranteed`.

---

## 5. Empirical Benchmarks: Mathematical Solvers vs. Grid Search

To measure the real-world performance advantage, the `sizing-bench` crate was executed across all 7 supported curve families on identical hardware (AMD Ryzen 9, Ubuntu 24.04 / Windows 11, Rust release profile).

The benchmark compared `optimal-sizing` against standard naive grid searches across four resolutions ($N = 10, 100, 1,000, 10,000$).

### 1. Wall-Clock Latency Benchmark

| Curve Family | `optimal-sizing` Algorithm | Library Latency (µs) | Grid $N=100$ Latency (µs) | Grid $N=10,000$ Latency (µs) | Real Speedup vs High-Precision Grid |
|---|---|:---:|:---:|:---:|:---:|
| **Uniswap v2 (CPMM)** | Closed-Form (Eq. 5) | **45 µs** | 86 µs | 9,947 µs | **221x faster** |
| **Uniswap v3 (CL)** | Piecewise Closed-Form | **45 µs** | 104 µs | 10,926 µs | **242x faster** |
| **Balancer Weighted** | Power-Law Closed-Form | **75 µs** | 165 µs | 17,996 µs | **240x faster** |
| **WooFi v2 (sPMM)** | Golden-Section + Check | **298 µs** | 75 µs | 9,104 µs | **30x faster** |
| **DODO PMM** | Golden-Section Search | **735 µs** | 962 µs | 107,330 µs | **146x faster** |
| **Curve StableSwap** | Newton's Method (Memoized) | **879 µs** | 1,790 µs | 156,315 µs | **177x faster** |
| **Curve CryptoSwap** | Dynamic Invariant Solver | **1,202 µs** | 1,924 µs | 162,065 µs | **135x faster** |

```
Execution Latency (Microseconds, Log Scale)
100,000 µs +-------------------------------------------------------+ [Grid N=10,000: 156ms]
           |                                                      * |
 10,000 µs +----------------------------------------------------*-+-+ [Grid N=1,000: 16ms]
           |                                                  *     |
  1,000 µs +----------------------------------------------*-+-------+ [optimal-sizing Curve: 0.8ms]
           |                                            *   |       |
    100 µs +----------------------------------------*---+---+-------+ [optimal-sizing CPMM: 0.045ms]
           |                                      * |   |   |       |
     10 µs +----------------------------------*---+---+---+---+-----+
           +----------------------------------+---+---+---+---+-----+
                                            10  100  1K  10K
                                           Grid Search Density (N)
```

### 2. Accuracy vs. Grid Density

Does a bot really need $N = 10,000$ grid evaluations? **Yes.**

The table below measures the relative error of a grid search compared to `optimal-sizing`'s exact mathematical solution $\Delta x^*$:

| Grid Density ($N$) | Average Relative Error ($\frac{|\Delta x_{\text{grid}} - \Delta x^*|}{\Delta x^*}$) | Max Profit Loss | Passes Within $10^{-4}$ (0.01%) Tolerance? |
|:---:|:---:|:---:|:---:|
| **10** | **9.50%** | Massive loss / Negative return | ❌ NO |
| **100** | **0.74%** | Substantial margin destruction | ❌ NO |
| **1,000** | **0.083%** | Leaves alpha on table | ❌ NO |
| **10,000** | **0.0046%** | Minimal alpha loss | ✅ YES (At a cost of 156 ms!) |

**The Takeaway**: To obtain institutional precision ($< 0.01\%$ error), a grid search requires 10,000 steps, taking over **150 milliseconds**. In block builder auctions (where block intervals are 12 seconds on Ethereum and 400 milliseconds on Solana/Arbitrum), 150 ms is an eternity. `optimal-sizing` delivers higher-than-grid precision in **0.045 to 0.879 milliseconds**.

---

## 6. Battle-Hardened Architecture & Numeric Precision

Mathematical equations on paper often crumble when deployed to production. `optimal-sizing` incorporates three critical engineering defenses:

### 6.1 The Float Trap: Why IEEE-754 Destroys MEV

Most Python and C++ sizing implementations use standard 64-bit floating-point numbers (`f64` / `double`). 

In automated market makers, pools frequently operate with extreme reserve imbalances (e.g., $10^7$ DAI vs $10^1$ WBTC, or large amplification factors $A = 2000$). When raising large pool invariants to powers during StableSwap Newton steps:

$$D_P = \frac{D^{n+1}}{n^n \prod x_i}$$

For a pool with $D \approx 3 \times 10^7$ and $n = 3$, $D^4 \approx 8 \times 10^{29}$. Standard `f64` maintains only **53 bits of precision** (~15 to 17 decimal digits). The least significant digits of your reserves are silently truncated. In Newton iterations, this causes **infinite oscillation cycles**, preventing convergence.

`optimal-sizing` enforces a strict invariant: **Zero floats across the entire codebase.** All public and internal math uses `rust_decimal::Decimal` (28–29 significant digits of exact fixed-point precision).

### 6.2 Fixed Execution Costs & Gas Domain Restrictions

A trade that is mathematically optimal against an AMM curve can be disastrously unprofitable in reality once blockchain gas is deducted.

`optimal-sizing` models fixed costs $C_{\text{gas}}$ directly inside the optimization domain. Let:

$$\Pi_{\text{net}}(\Delta x) = \Pi_{\text{gross}}(\Delta x) - C_{\text{gas}}$$

Because subtracting a constant $C_{\text{gas}}$ does not alter the derivative ($\Pi_{\text{net}}'(\Delta x) = \Pi_{\text{gross}}'(\Delta x)$), the unconstrained critical point $\Delta x^*$ remains unchanged! 

However, the **profitable domain** shrinks from $\{\Delta x : \Pi_{\text{gross}}(\Delta x) > 0\}$ to:

$$\mathcal{D}_{\text{profitable}} = \{\Delta x : \Pi_{\text{gross}}(\Delta x) > C_{\text{gas}}\}$$

If $\Pi_{\text{gross}}(\Delta x^*) \le C_{\text{gas}}$, the entire curve is underwater. `sizing-core` rejects the trade upfront with `SizingError::NoProfitableSize`, saving the searcher from submitting a reverting transaction.

```rust
pub struct SizingConstraints {
    /// Fixed execution cost (gas, protocol overhead) in output asset units.
    /// Not optional. Use Decimal::ZERO if intentionally unconstrained.
    pub fixed_cost: Decimal,

    /// Upper capacity bound on delta_in (e.g. wallet balance, flash loan limit).
    pub max_size: Option<Decimal>,
}
```

### 6.3 Derivative Step Sizing and Catastrophic Cancellation

In StableSwap and PMM numerical root-finding, the marginal derivative $\Pi'(\Delta x)$ is evaluated via finite differences:

$$\Pi'(\Delta x) \approx \frac{\text{quote}(\Delta x + h) - \text{quote}(\Delta x - h)}{2h}$$

If step size $h$ is chosen too small (e.g. $h \approx \epsilon_{\text{tol}} \approx 10^{-6}$), floating/fixed subtraction cancels significant digits, yielding random numerical noise. If $h$ is too large, truncation error dominates.

Following classical numerical analysis, `optimal-sizing` fixes the finite-difference step to the optimal error-minimizing scale:

$$h = x_{\text{reserve}} \cdot \sqrt{\text{TOLERANCE}} \approx x_{\text{reserve}} \cdot 10^{-3}$$

This ensures stable, monotonically decreasing error bounds across all Newton iterations.

---

## 7. Multi-Hop Heterogeneous Routing (`sizing-router`)

Real-world arbitrage frequently spans multi-hop paths (e.g., swapping Token A on Uniswap v2 $\to$ Token B on Curve 3pool $\to$ Token C on Balancer).

### The Common Anti-Pattern: Greedy Independent Sizing
Most routers attempt to size multi-hop routes by independently optimizing each pool against the global price. This fails because:
$$\text{Output of Leg 1} = \text{Input of Leg 2}$$

The marginal price of Leg 2 changes dynamically based on the exact quantity delivered by Leg 1. Independent sizing produces massive slippage mismatches.

### The `sizing-router` Approach: Joint Composite Optimization
The `sizing-router` crate sizes arbitrary $N$-hop heterogeneous routes as an integrated composite function:

$$\text{RouteQuote}(\Delta x) = \text{Leg}_N\Big(\dots \text{Leg}_2\big(\text{Leg}_1(\Delta x)\big)\dots\Big)$$

$$\max_{\Delta x \ge 0} \quad \Pi_{\text{route}}(\Delta x) = \text{RouteQuote}(\Delta x) - P_{\text{route}} \cdot \Delta x - C_{\text{gas}}$$

```mermaid
flowchart LR
    In[Input Asset Δx] --> Leg1[Leg 1: Uniswap v2 CPMM]
    Leg1 -->|Intermediate Output| Leg2[Leg 2: Curve 3pool StableSwap]
    Leg2 -->|Intermediate Output| Leg3[Leg 3: Balancer Weighted]
    Leg3 --> Out[Final Asset Quote]
    Out --> JointOpt[sizing-router Joint Optimizer]
    JointOpt --> Result[Optimal Route Size & ComposedFrom Tier]
```

Because function composition does not preserve concavity (a concave function composed with a concave function is **not** necessarily concave), `sizing-router` evaluates the composite path via an adaptive bounded golden-section search over Leg 1's input space, returning `GuaranteeTier::ComposedFrom`.

---

## 8. Practical Implementation Guide

Here is how to integrate `optimal-sizing` into your trading infrastructure today.

### 8.1 Rust Implementation (`sizing-core`)

Add the dependencies to your `Cargo.toml`:
```toml
[dependencies]
sizing-core = { git = "https://github.com/Xtley001/optimal-sizing" }
rust_decimal = "1.36"
rust_decimal_macros = "1.36"
```

#### Sizing a Uniswap v2 Pool with Gas Constraints:
```rust
use rust_decimal_macros::dec;
use sizing_core::curves::Cpmm;
use sizing_core::{SizingAlgorithm, SizingConstraints, GuaranteeTier};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize Uniswap v2 Pool (1.2M USDC, 800 WETH, 0.3% fee)
    let pool = Cpmm::new(
        dec!(1_200_000), // reserve_x (USDC)
        dec!(800_000),   // reserve_y (WETH)
        dec!(0.997),     // fee_retention (1 - 0.003)
    )?;

    // 2. Define Execution Constraints
    let constraints = SizingConstraints {
        fixed_cost: dec!(15.00),    // 15 WETH equivalent in gas
        max_size: Some(dec!(100_000)), // Max 100k USDC wallet capital
    };

    // 3. Define Reference Market Price (P = 0.60 USDC per WETH)
    let market_price = dec!(0.60);

    // 4. Compute Provably Optimal Trade Size
    let result = pool.optimal_size(market_price, constraints)?;

    println!("=== OPTIMAL SIZING RESULT ===");
    println!("Optimal Input Size : {} USDC", result.optimal_delta);
    println!("Expected Profit    : {} WETH", result.expected_profit);
    println!("Guarantee Tier     : {:?}", result.guarantee_tier);
    println!("Iterations Needed  : {:?}", result.iterations); // None (Closed form!)

    assert_eq!(result.guarantee_tier, GuaranteeTier::ProvenOptimal);
    Ok(())
}
```

---

### 8.2 Python Implementation (`sizing-py`)

For algorithmic traders and quantitative researchers running Python research stacks (pandas, polars, backtesting.py), `sizing-py` exposes compiled C-extension speed with native `decimal.Decimal` safety.

Install via pip (or build from repo via `maturin`):
```bash
cd crates/sizing-py
maturin develop --release
```

#### Python Sizing Script:
```python
from decimal import Decimal
import sizing_py

# 1. Initialize Curve 3pool (DAI / USDC / USDT)
# reserves: [15M DAI, 8M USDC, 9.5M USDT], Amplification A = 100, fee = 0.04%
stableswap = sizing_py.PyStableSwap(
    reserves=[Decimal("15000000"), Decimal("8000000"), Decimal("9500000")],
    amplification=Decimal("100"),
    fee_retention=Decimal("0.9996")
)

# 2. Constraints: $25 Gas Cost
constraints = sizing_py.PySizingConstraints(
    fixed_cost=Decimal("25.00"),
    max_size=Decimal("5000000") # Cap at 5M
)

# 3. Market Reference Price (External exchange quoting 0.9985)
ref_price = Decimal("0.9985")

# 4. Compute Trade Size
result = stableswap.optimal_size(ref_price, constraints)

print(f"Optimal Trade Size : {result.optimal_delta:.4f} DAI")
print(f"Expected Net Profit: {result.expected_profit:.4f} USDC")
print(f"Guarantee Tier     : {result.guarantee_tier}")
print(f"Newton Iterations  : {result.iterations}")
```

---

### 8.3 Live Ethereum Mainnet Integration (`sizing-integration`)

How does `optimal-sizing` ingest on-chain reserves? The `sizing-integration` crate connects directly to Ethereum Mainnet using `Multicall3` batching:

```bash
# Set your Ethereum RPC and execute
RPC_URL="https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY" \
cargo run -p sizing-integration --release
```

#### What It Does Under the Hood:
1. **Multicall3 Batching**: Instead of 10 sequential HTTP roundtrips, it batches reserve reads into **3 optimized multicalls**:
   - Call 1: Uniswap v2 `getReserves()`, `token0()`, `token1()`.
   - Call 2: `token0.decimals()`, `token1.decimals()`.
   - Call 3: Curve 3pool `balances(0..2)`, `A()`, `fee()`.
2. Normalizes raw integer token amounts to true decimal representations.
3. Feeds live mainnet liquidity into `sizing-core` in real time.

---

### 8.4 Command-Line Sizing (`sizing-cli`)

For shell scripts, DevOps sanity checks, and rapid manual verification, `sizing-cli` provides an instant command-line interface:

```bash
# Size a Uniswap v2 mispricing in 1 millisecond
sizing-cli cpmm \
  --x 1200000 \
  --y 800000 \
  --fee 0.997 \
  --price 0.60 \
  --fixed-cost 12.50

# Output:
# optimal_delta: 63201.877941580268260688898696
# expected_profit: 1978.746971608191627795187204
# guarantee_tier: ProvenOptimal
```

---

## 9. Frequently Asked Questions (FAQ)

### Q1: Why shouldn't I just use standard convex optimization solvers like SciPy or GSL?
**Answer**: General-purpose optimizers (like SciPy's `minimize` or GSL) rely on floating-point arithmetic (`float64`), which suffers from precision truncation near AMM reserve boundaries. Furthermore, general-purpose solvers incur massive runtime overhead (often taking 5 to 50 ms in Python). `optimal-sizing` executes dedicated, analytical, and domain-bounded routines in Rust in **45 to 800 microseconds** with 28-digit fixed-point precision.

### Q2: Does concavity survive multi-hop route composition?
**Answer**: **No.** Even if every individual pool on a route is strictly concave (e.g., two Uniswap v2 pools), the composition of two concave functions $f(g(x))$ is generally **not concave** unless $f$ is non-decreasing and $g$ satisfies specific second-order convexity requirements. That is why `sizing-router` labels composite routes as `GuaranteeTier::ComposedFrom` and applies an adaptive bounded golden-section search rather than asserting global closed-form optimality.

### Q3: How does the library handle gas price volatility?
**Answer**: `sizing-core` provides a dedicated helper:
```rust
gas_aware_fixed_cost(
    gas_price_wei, 
    gas_units_per_swap, 
    native_token_price_in_output_units
)
```
This converts real-time base fees and priority tips into output asset units before sizing, guaranteeing that sudden gas spikes dynamically reduce optimal trade sizes to prevent underwater transactions.

### Q4: Can I use `optimal-sizing` on Solana, Arbitrum, or Monad?
**Answer**: **Yes.** The core library (`sizing-core`) is completely zero-I/O and chain-agnostic. It has zero dependencies on ethers-rs, alloy, or web3. As long as you can read pool reserves into numbers (via Solana RPC, Monad RPC, or local shared memory), `sizing-core` will calculate optimal sizing instantly.

### Q5: How is this licensed?
**Answer**: `optimal-sizing` is licensed under the permissive [MIT License](https://github.com/Xtley001/optimal-sizing/blob/main/LICENSE), free for commercial and proprietary trading bot deployments.

---

## 10. Conclusion & Getting Started

In algorithmic trading, **mathematical rigor is an edge**. Continuing to size high-frequency trades using blind bisection or slow grid search is an unnecessary tax on your PnL.

By moving to closed-form analytical solutions and provably convergent numerical methods:
- You drop sizing latency from **100+ milliseconds to 45 microseconds**.
- You recover the **0.74% alpha margin** lost to grid quantization.
- You eliminate **floating-point convergence failures** on StableSwap pools.
- You receive explicit **guarantee tiers** with every calculation.

### Resources & Next Steps
- **GitHub Repository**: [https://github.com/Xtley001/optimal-sizing](https://github.com/Xtley001/optimal-sizing)
- **Technical Whitepaper**: [Read the Full Mathematical Derivations](https://github.com/Xtley001/optimal-sizing/blob/main/docs/whitepaper.md)
- **Architecture Specification**: [System Architecture & Boundaries](https://github.com/Xtley001/optimal-sizing/blob/main/docs/ARCHITECTURE.md)
- **Crate Installation**:
  ```bash
  cargo add sizing-core rust_decimal rust_decimal_macros
  ```
- **Contribute**: We welcome PRs for additional curve families (Curve v2 crypto-pools, Maverick v2, ambient/crocswap). Check out [`ROADMAP.md`](https://github.com/Xtley001/optimal-sizing/blob/main/ROADMAP.md) for open initiatives.

---

*If you found this article valuable, star the repo on [GitHub](https://github.com/Xtley001/optimal-sizing), share it with your fellow quant researchers, and leave a clap on Medium!*
