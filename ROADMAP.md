# Roadmap: What's Genuinely Still Proposed

Everything in this file is unstarted. For what's already built, see [`CHANGELOG.md`](./CHANGELOG.md) (past tense, what shipped, with verification status per item). Past tense goes in `CHANGELOG.md`; only genuinely future-tense proposals belong here.

## Real-time / streaming sizing

Every current entry point is one-shot: fetch state once, size once, print once. A bot needs to react to every new block. A `--watch` mode (or a new `sizing-watch` binary) that subscribes to new blocks — or just polls on an interval — and re-sizes automatically would close that gap.

**Why this hasn't been attempted:** needs a live WebSocket subscription to a real chain. No environment this project has run in so far has had that kind of network access — this isn't a difficulty problem, it's an access problem, and writing the code without ever running it against a real feed would produce something untested at exactly the point (a persistent connection, reconnection handling, partial-message handling) where untested code is most likely to be wrong.

## Historical backtesting harness

Given a source of historical pool state (a subgraph, an indexer, or archived block data), replay real historical mispricings through `sizing-core` and check: would this have actually been profitable, net of the fees and gas it claims to account for? This is the most credible way to validate the *model* against reality — the existing proptests prove the *math* is implemented correctly, not that the model matches how real pools actually behave.

**Why this hasn't been attempted:** needs a real historical data source, unreachable from every environment this project has run in so far. A harness built against fabricated "historical" data would produce a plausible-looking but meaningless result — worse than not building it, since a meaningless-but-plausible backtest is more likely to be trusted than an absent one.

## Tier 2 items, not yet attempted

- **Full sizing-cli route support.** `sizing-cli route2-hop-cpmm` only wires up CPMM-CPMM routes; `sizing-router::Leg` supports `StableSwap`/`Pmm` legs too, but exposing every combination through CLI flags hasn't been done.
- **Curve-exact `SlippageBound`.** The current version applies a staleness fraction directly to an already-quoted output rather than re-solving the curve at hypothetically-moved reserves — see `slippage.rs`'s module doc for exactly what a more precise version would need (a `with_moved_reserves`-style method no curve exposes today).
- **Calibrated gas-units-per-swap figures.** `gas_aware_fixed_cost` is a pure unit-conversion helper; it takes `gas_units_per_swap` as a required argument rather than defaulting it, because no environment this project has run in has had access to real transaction receipts to calibrate a default from.

## Tier 3 — ambitious, deliberately deferred

- **Generic PMM abstraction + a curve-family plugin system.** `docs/ARCHITECTURE.md` explicitly scopes this to WooFi's formula only, "until there is a second concrete PMM formula to generalize from" — DODO's PMM is exactly that second data point. Once two real PMM formulas exist side by side, the shared structure (and what's genuinely WooFi-specific vs. general) becomes visible, and a proper `trait PmmModel` abstraction becomes an informed decision instead of a guess.
- **Portfolio/basket-level sizing.** Size a coordinated set of trades across several *independent* opportunities simultaneously, subject to a shared capital constraint. A genuinely different optimization problem (constrained allocation across several `SizingResult`s, not a single curve's sizing problem) — probably its own crate rather than living in `sizing-core`.
- **Formal/property verification of the guarantee proofs themselves.** The three `GuaranteeTier`s currently rest on a hand-derived proof (CPMM), stated-but-runtime-checked regularity conditions (StableSwap), and a sampled runtime heuristic (PMM, tightened after a later review but still a heuristic). An SMT-solver-backed or exhaustive-interval check that CPMM's concavity proof actually holds across `Decimal`'s representable range would strengthen `ProvenOptimal` from "proven on paper, trusted in code" to "proven and checked."

## Suggested order, if picked up

1. Streaming/`--watch` mode or the backtesting harness — whichever a real deployment environment (with either live RPC or historical data access) becomes available for first, since both are blocked on access, not difficulty.
2. The smaller Tier 2 gaps (full CLI route support, curve-exact slippage bound) — small, self-contained, no external access needed.
3. Tier 3, once its own stated blocker for each item is actually resolved (a second real PMM formula existing, a real portfolio-sizing use case, someone with SMT-solver expertise picking it up) — not before, per this project's own "don't invent, don't assume" discipline.

Whichever gets picked up first, the same discipline that shaped everything else in this repo should still apply: don't invent a formula or a default that isn't sourced from somewhere real, state exactly what's proven versus assumed versus sampled, verify claims (including this document's own, and including an external reviewer's) against the actual code before trusting them, and log the decision in the doc that owns it rather than only in a commit message.
