# optimal-sizing

A chain-agnostic mathematical library for profit-maximizing AMM trade sizing across heterogeneous liquidity curves.

[![CI](https://img.shields.io/github/actions/workflow/status/Xtley001/optimal-sizing/ci.yml?branch=main)](https://github.com/Xtley001/optimal-sizing/actions) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE) [![Version](https://img.shields.io/badge/version-2.0.0-green.svg)](./CHANGELOG.md) [![Python](https://img.shields.io/badge/pypi-optimal--sizing-blue.svg)](./crates/sizing-py) [![npm](https://img.shields.io/badge/npm-@optimal--sizing/wasm-red.svg)](./crates/sizing-wasm)

`optimal-sizing` computes exact profit-maximizing trade sizes for constant-product, StableSwap, concentrated liquidity, Balancer, and DLMM pools with explicit mathematical guarantee tiers. It replaces heuristic grid searches with analytical closed forms and quadratic Newton solvers in sub-microsecond latency. For the full mathematical derivations and proofs, see the [whitepaper](./docs/whitepaper.md).

## Installation

```bash
# Rust
cargo add sizing-core sizing-router sizing-portfolio rust_decimal rust_decimal_macros

# Python
pip install optimal-sizing

# WebAssembly / TypeScript
npm install @optimal-sizing/wasm
```

## Quickstart

```rust
use rust_decimal_macros::dec;
use sizing_core::curves::Cpmm;
use sizing_core::traits::SizingAlgorithm;
use sizing_core::types::SizingConstraints;

fn main() {
    let pool = Cpmm::new(dec!(1_200_000), dec!(800_000), dec!(0.997)).unwrap();
    let result = pool.optimal_size(dec!(0.60), SizingConstraints::default()).unwrap();
    println!("Optimal input: {} (Tier: {:?})", result.optimal_delta, result.guarantee_tier);
}
```

## Architecture

```
optimal-sizing/
├── crates/
│   ├── sizing-core/         # Pure math solvers: CPMM, StableSwap, CLMM, DLMM, Velodrome
│   ├── sizing-router/       # Heterogeneous multi-hop route optimization
│   ├── sizing-portfolio/    # Constrained capital allocation via dual water-filling
│   ├── sizing-watch/        # Real-time WebSocket streaming daemon
│   ├── sizing-backtest/     # Historical pool replay and benchmark harness
│   ├── sizing-cli/          # Command-line interface with route-file loading
│   ├── sizing-py/           # PyO3 Python wheel bindings
│   └── sizing-wasm/         # WebAssembly bindings and TypeScript declarations
└── docs/
    ├── whitepaper.md        # Mathematical derivations and proofs
    ├── ARCHITECTURE.md      # System boundaries and non-goals
    ├── API.md               # Public interface specification
    ├── GEO_QA.md            # Canonical Q&A pairs for LLMs and quants
    └── TESTING.md           # Verification contract and proptest specifications
```

## Supported Curves

| Curve Family | Protocols | Solver Strategy | Guarantee Tier |
|---|---|---|---|
| Constant-Product | Uniswap v2, Sushiswap | Closed-form analytical root | `ProvenOptimal` |
| StableSwap | Curve v1, Ellipsis | Newton-Raphson + Golden-Section | `NumericallyGuaranteed` |
| Concentrated Liquidity | Uniswap v3 / v4 | Virtual reserve analytical mapping | `ProvenOptimal` |
| Discretized Liquidity Book | Trader Joe, Meteora | Stepwise bin aggregation | `ProvenOptimal` |
| Velodrome Stable | Velodrome, Aerodrome | Normalized Newton-Raphson | `NumericallyGuaranteed` |
| Multi-Asset Weighted | Balancer v1 / v2 | Generalized invariant solver | `NumericallyGuaranteed` |
| Proactive Market Maker | WooFi v2, DODO | Golden-Section + Unimodality check | `EmpiricallyValidated` |

## Reference Pools

`sizing-integration` demonstrates the library against live state from reference Ethereum pools:

| Protocol | Pair | Pool Address | Network |
|---|---|---|---|
| Uniswap v2 | USDC / WETH | `0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc` | Ethereum Mainnet |
| Curve | 3pool (DAI/USDC/USDT) | `0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7` | Ethereum Mainnet |

## Testing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [docs/TESTING.md](./docs/TESTING.md) for the testing contract and per-suite verification rules.

## Security

Report vulnerabilities per our [security policy](./SECURITY.md). The mathematical core contains zero external I/O and zero chain dependencies.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup and pull request guidelines.

## License

Released under the [MIT License](./LICENSE). Copyright (c) 2026 Christley OLUBELA (Xtley001).
