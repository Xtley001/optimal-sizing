# optimal-sizing

A chain-agnostic Rust library for profit-maximizing trade sizing against CPMM, StableSwap, PMM, Concentrated Liquidity, Balancer, DODO, and CryptoSwap curves.

[![CI](https://img.shields.io/github/actions/workflow/status/Xtley001/optimal-sizing/ci.yml?branch=main)](https://github.com/Xtley001/optimal-sizing/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![status: pre-1.0](https://img.shields.io/badge/status-pre--1.0-orange.svg)](./docs/whitepaper.md)

Most sizing code in the wild is a naive grid search or an unexamined bisection loop, with no stated answer to "how do you know this is actually optimal." `optimal-sizing` answers that directly: constant-product sizing is closed-form and provably optimal; StableSwap sizing is Newton's-method-solved with stated convergence conditions; PMM sizing is golden-section-solved conditional on a runtime unimodality check rather than an assumed one. Every `SizingResult` carries a `GuaranteeTier` telling you which is which. For the full derivations and security considerations, see the [whitepaper](./docs/whitepaper.md). For what's been built beyond the core three curves — multi-hop routing, concentrated liquidity, Balancer, DODO, CryptoSwap, WASM/Python bindings, and more — see [`CHANGELOG.md`](./CHANGELOG.md). For what's still proposed, see [`ROADMAP.md`](./ROADMAP.md).

## Installation

```bash
# Once published to crates.io:
cargo add sizing-core rust_decimal rust_decimal_macros

# Before then, or to track main directly:
cargo add rust_decimal rust_decimal_macros
cargo add sizing-core --git https://github.com/Xtley001/optimal-sizing
```

`rust_decimal`/`rust_decimal_macros` are needed directly (not just transitively through `sizing-core`) because the `dec!` macro used below expands to a call into the `rust_decimal` crate in your own crate's namespace.

New to this repo? See [`SETUP.md`](./SETUP.md) for pushing to GitHub, building/running locally or in Codespaces, and publishing to crates.io.

## Quickstart

```rust
use sizing_core::curves::Cpmm;
use sizing_core::{SizingAlgorithm, SizingConstraints};
use rust_decimal_macros::dec;

fn main() {
    let pool = Cpmm::new(dec!(1_200_000), dec!(800_000), dec!(0.997)).unwrap(); // (x, y, fee_retention)

    let constraints = SizingConstraints {
        fixed_cost: dec!(12.50), // gas/execution cost, denominated in the output asset
        max_size: None,
    };

    // reference_price = P. This pool's own marginal price at zero size is
    // ~0.6647 (see whitepaper.md § 5.1's worked example) — 0.60 is
    // profitably mispriced against it.
    let result = pool.optimal_size(dec!(0.60), constraints).unwrap();

    println!("{:?} {:?}", result.optimal_delta, result.guarantee_tier);
    // 63201.877941580268260688898696 ProvenOptimal
}
```

## Architecture

```
optimal-sizing/
├── crates/
│   ├── sizing-core/         # the library — pure math, zero I/O, zero chain deps
│   ├── sizing-router/       # multi-hop heterogeneous routing across arbitrary curves
│   ├── sizing-cli/          # thin CLI over sizing-core / sizing-router
│   ├── sizing-watch/        # real-time streaming sizing daemon across DEX pools
│   ├── sizing-wasm/         # wasm-bindgen boundary wrapper
│   ├── sizing-py/           # PyO3 boundary wrapper
│   ├── sizing-bench/        # convergence/accuracy benchmarks vs. naive grid search
│   └── sizing-integration/  # reference integration against live Ethereum mainnet state
└── docs/
    ├── whitepaper.md        # full derivations, proofs, convergence conditions
    ├── ARCHITECTURE.md      # module boundaries, data flow, non-goals
    ├── API.md               # exact public interface — types, traits, error variants
    └── TESTING.md           # the testing contract
```

See [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) for module boundaries and [`docs/API.md`](./docs/API.md) for the full public interface.

## Reference pools

`sizing-integration` demonstrates the library against live state from these two pools:

| Protocol | Pool | Address | Chain |
|---|---|---|---|
| Uniswap v2 | USDC/WETH | `0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc` | Ethereum Mainnet |
| Curve | 3pool (DAI/USDC/USDT) | `0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7` | Ethereum Mainnet |

```bash
RPC_URL=<your Ethereum mainnet RPC endpoint> cargo run -p sizing-integration
```

Read-only, sends no transactions, fails loudly if `RPC_URL` is unset. See [`SECURITY.md`](./SECURITY.md) for the known-risk assumptions to verify before trusting its output.

## Testing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [`docs/TESTING.md`](./docs/TESTING.md) for the full testing contract and per-suite commands. Enforced on every push/PR via [CI](./.github/workflows/ci.yml).

## Benchmarks

```bash
cargo run -p sizing-bench --release
```

Regenerates [`docs/benchmark-report.md`](./docs/benchmark-report.md) with real, freshly measured wall-clock and accuracy comparisons against a naive grid-search baseline, for all three curve families.

## Security

`sizing-core` has no external I/O and no chain dependency (enforced in CI via `cargo tree`). It has not been audited. See [`SECURITY.md`](./SECURITY.md) for scope, known unverified assumptions, and how to report a vulnerability.

## Contributing

See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for dev setup and PR guidelines. See [`CHANGELOG.md`](./CHANGELOG.md) for what's been built and [`ROADMAP.md`](./ROADMAP.md) for what's still proposed, ranked by fit and value. Setting this repo up on GitHub/Codespaces for the first time? See [`SETUP.md`](./SETUP.md).

## License

Released under the [MIT License](./LICENSE).
