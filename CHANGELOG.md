# Changelog

What's actually built, in past tense, as it was built — as opposed to [`ROADMAP.md`](./ROADMAP.md), which is only what's genuinely still proposed.

## Code review fixes

A later external review of the codebase (a static read, no compiler available to the reviewer) found several real issues. Each was independently re-verified against the actual code — and in two cases corrected or narrowed — before fixing. Full writeup: [`docs/ARCHITECTURE.md § Fixes from a later code review`](./docs/ARCHITECTURE.md).

- Fixed: `StableSwap::quote()` re-solving its Newton's-method invariant `D` from scratch on every call, including the ~40-80 internal calls `optimal_size`'s own search made per invocation. Memoized within a single `optimal_size` call (provably safe — not cached across separate calls, since reserves are public/mutable). Measured ~1.5x speedup for a representative pool; proven via an instrumented call-count test, not just claimed.
- Fixed: `StableSwap::optimal_size`'s finite-difference derivative step was scaled to the same tolerance as `quote()`'s own convergence noise floor, risking catastrophic cancellation. Widened to the standard `sqrt(tolerance)` rule of thumb.
- Fixed: `Pmm::check_unimodality` only checked that curvature never *flipped* sign, not which sign it was — a search interval confined to the analytically-convex reverse regime (`reverse_sell_base`) was incorrectly reported as `unimodality_confirmed: true`. Fixed to require non-positive (concave) curvature. This surfaced and fixed two further bugs in `pmm_proptest.rs`: a generator that didn't actually produce the "near-balanced" pools its own comment claimed, and a grid-search cross-check tolerance tighter than the grid's own quantization resolution.
- Fixed: `GOLDEN_RATIO_INV` was hand-truncated to 10 digits in a library that otherwise uses `Decimal` specifically to avoid float-style truncation. Extended to full precision, computed via `Decimal::sqrt()` itself rather than transcribed.
- Fixed: `Pmm::optimal_size`'s search upper bound was a bare, undocumented `base_reserve * 1000` — an admitted invented default. Replaced with `search_upper_bound()`, derived from the curve's own pole and capacity cap via the same doubling-bracket pattern used elsewhere in this codebase. Extracting this into a shared method also fixed a second bug it surfaced: two tests had silently drifted from the real implementation by independently re-deriving the old formula.
- Corrected, not fixed: the review's claim that root-level docs ship inside the published crates.io package. Verified with `cargo package --list`: they don't (they live outside every crate's own directory). The review conflated the GitHub repo with the published package.
- Ran `cargo clippy --workspace --all-targets` for the first time (previously only `cargo build`/`cargo test` had been checked) — found and fixed 2 real lints: a redundant borrow in `sizing-integration`, and a justified-but-unsuppressed `too_many_arguments` in `sizing-py`'s PMM binding (inherent to `Pmm`'s own parameter count, now explicitly `#[allow]`'d with a comment explaining why). `sizing-core` itself — the audit-critical crate — was already clean.

## Multi-hop routing, concentrated liquidity, and language bindings

Verification level differs per item — see [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) for what's actually been run end-to-end vs. only `cargo check`-verified.

- **Multicall batching** (`sizing-integration`) — 10 separate `eth_call` round-trips reduced to 3 (not 1 — Uniswap's `decimals()` genuinely depends on `token0()`/`token1()`'s result first, a real data dependency the original design didn't account for).
- **Multi-hop routing (2-hop)** — new `sizing-router` crate, `GuaranteeTier::ComposedFrom` added to `sizing-core`. A joint search over leg-1 input size, not two independently-computed per-leg results composed after the fact.
- **Concentrated liquidity** — new `ConcentratedLiquidity` curve in `sizing-core`, single tick range. Reuses `Cpmm`'s exact closed form on Uniswap v3's "virtual reserves," earning genuine `ProvenOptimal`, cross-validated directly against `Cpmm`.
- **WASM bindings** — new `sizing-wasm` crate. `cargo check` passes against the host target; not yet verified against the actual `wasm32-unknown-unknown` target.
- **Python bindings** — new `sizing-py` crate. Built and the compiled `.so` loaded from a real Python 3.12 interpreter and exercised end-to-end — the most-verified of the new crates.
- **`sizing-cli`** — new binary, all subcommands run for real from the command line.
- **`SlippageBound`** / **`gas_aware_fixed_cost`** — small, pure-math additions to `sizing-core`, both explicitly scoped down from what a fully precise version would do (see their own module docs).
- **Multi-chain config** (`sizing-integration`) — pool addresses now configurable via `UNISWAP_POOL`/`CURVE_POOL` env vars; `RPC_URL` already worked for any EVM chain.

## Initial implementation

The core library: `Cpmm` (closed-form sizing), `StableSwap` (Newton's-method-solved), and `Pmm` (golden-section search conditional on a runtime unimodality check), each carrying an explicit `GuaranteeTier`. The `NetProfit` fixed-cost/capacity layer, the initial `sizing-bench` and `sizing-integration` crates, and the documentation set (`docs/whitepaper.md`, `docs/API.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`) were built and reconciled against the shipped code during this phase.
