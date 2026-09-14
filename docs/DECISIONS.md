# Architectural & Mathematical Decision Log (ADR)

> **Authority:** This document logs every major architectural, mathematical, and algorithmic decision made across `optimal-sizing`, including alternatives considered and consequences locked in.

---

## ADR-001: Selection of `rust_decimal::Decimal` as Universal Numeric Type
* **Date:** 2026-08-30
* **Decision:** All public APIs and internal solvers across `sizing-core` and `sizing-router` use fixed-point `rust_decimal::Decimal` (28–29 significant decimal digits). No floating-point types (`f32`, `f64`) are permitted.
* **Alternatives Considered:**
  - `f64`: Fast hardware floats.
  - `alloy_primitives::U256`: Raw 256-bit EVM integers.
* **Reason:** Floating-point rounding error compounds across Newton's method and golden-section iterations, creating unpredictable edge errors near tick and reserve boundaries. `U256` ties the core library to EVM integer representation and complicates fractional exponentiation ($w_i/w_o$, sqrt). `Decimal` provides exact decimal representation, pure chain-agnosticism, and high performance.
* **Consequences:** All language bindings (Python, WASM) pass strings or native Decimal representations across the boundary.

---

## ADR-002: Explicit `GuaranteeTier` on Every Sizing Result
* **Date:** 2026-08-30
* **Decision:** Every `SizingResult` carries a strongly typed `GuaranteeTier` enum (`ProvenOptimal`, `NumericallyGuaranteed`, `EmpiricallyValidated`, `ComposedFrom`).
* **Alternatives Considered:**
  - Returning a bare `Decimal` trade size.
  - Documenting convergence behavior solely in README/docstrings.
* **Reason:** In quantitative DeFi, treating heuristic bisection and proven closed-form optima as identical leads to uncalculated financial risk. Attaching the guarantee directly to the return type prevents callers from confusing empirical heuristics with proven concavity.
* **Consequences:** Multi-hop routes return `ComposedFrom` rather than inheriting `ProvenOptimal` because multi-hop composition does not guarantee preservation of joint concavity.

---

## ADR-003: StableSwap Newton Invariant $D$ Memoization per `optimal_size` Call
* **Date:** 2026-08-30
* **Decision:** In `StableSwap::optimal_size`, invariant $D$ is solved exactly once and threaded through the search loop via `quote_with_d(&self, delta, d)`.
* **Alternatives Considered:**
  - Solving $D$ inside every `quote()` call (40–80 redundant 255-iteration solves per sizing call).
  - Caching $D$ permanently on the `StableSwap` struct.
* **Reason:** $D$ depends only on pool reserves and amplification, not the candidate trade size $\Delta x$. Within an immutable borrow of `&self`, Rust guarantees reserves cannot change, making call-local memoization provably safe without the staleness risks of struct-level caching.
* **Consequences:** Measured ~1.5x end-to-end speedup on StableSwap sizing with zero staleness risk.

---

## ADR-004: Joint Multi-Hop Route Optimization vs. Independent Leg Composition
* **Date:** 2026-08-30
* **Decision:** `sizing-router` evaluates composite route profit $Profit(x) = quote_N(\ldots quote_1(x)) - P \cdot x - c$ directly via golden-section search over input size $x$, rather than sizing each leg independently and chaining outputs.
* **Alternatives Considered:**
  - Sizing Leg 1, then sizing Leg 2 with Leg 1's output.
  - Intermediate synthetic reference price derivation.
* **Reason:** Leg 1's marginal return varies non-linearly with Leg 2's price impact. Independent leg sizing misses the true joint optimum.
* **Consequences:** Route sizing requires evaluating composite quotes along the bracketed search domain.

---

## ADR-005: Multi-Tick Concentrated Liquidity as Piecewise Virtual Reserves
* **Date:** 2026-08-30
* **Decision:** Multi-tick Uniswap v3/v4 positions are modeled as contiguous piecewise constant-product curves on virtual reserves $L/\sqrt{p}, L\sqrt{p}$.
* **Alternatives Considered:**
  - Continuous float polynomial approximation.
  - Full EVM tick bitmap simulation in Rust.
* **Reason:** Piecewise virtual reserves retain exact closed-form CPMM solvability on each contiguous segment without EVM execution overhead.
* **Consequences:** Returns `GuaranteeTier::ProvenOptimal` across initialized tick spans.
