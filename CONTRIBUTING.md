# Contributing

## Development Setup

Clone the repository and build all workspace members:

```bash
git clone https://github.com/Xtley001/optimal-sizing.git
cd optimal-sizing
cargo build --workspace
```

Prerequisites:
- Rust toolchain (`stable`) via [rustup](https://rustup.rs).
- No external system dependencies required for `sizing-core`, `sizing-router`, or `sizing-portfolio`.

## Pull Request Process

Every pull request must pass the automated verification contract before review:

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Pull request requirements:
- **Scope:** Limit PRs to a single curve, algorithm, or crate.
- **Documentation:** Update [docs/API.md](./docs/API.md), [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md), or [docs/whitepaper.md](./docs/whitepaper.md) in the same PR as code changes.
- **Tests:** Add unit tests and proptest property coverage for any new mathematical or numerical routine per [docs/TESTING.md](./docs/TESTING.md).

## Code Standards

- **Zero Floating-Point Math:** `sizing-core` uses `rust_decimal::Decimal` exclusively. No `f32` or `f64` in numerical logic.
- **No Invented Defaults:** Numerical constants must cite formal specifications, published contracts, or explicit derivations.
- **Guarantee Tiers:** Every sizing routine must return an explicit `GuaranteeTier` (`ProvenOptimal`, `NumericallyGuaranteed`, or `EmpiricallyValidated`).

## Issue Reporting

- **Bug Reports:** Open a GitHub Issue with pool reserves, prices, expected versus actual outputs, and minimum reproducible code.
- **Security Findings:** Review [SECURITY.md](./SECURITY.md) for private vulnerability disclosure.
