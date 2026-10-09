# Setup Guide

## Prerequisites

- Rust toolchain (`stable`) via [rustup](https://rustup.rs).
- Python 3.9+ and [maturin](https://github.com/PyO3/maturin) (optional, for `sizing-py`).
- [wasm-pack](https://rustwasm.github.io/wasm-pack/) and Node.js 18+ (optional, for `sizing-wasm`).
- [GitHub CLI](https://cli.github.com) (`gh`) (optional, for remote setup).

## Local Development

```bash
# Clone and enter directory
git clone https://github.com/Xtley001/optimal-sizing.git
cd optimal-sizing

# Build all workspace crates
cargo build --workspace

# Run full test suite
cargo test --workspace

# Validate linter standards
cargo clippy --workspace --all-targets -- -D warnings
```

## Git Remote Configuration

```bash
# Option A: GitHub CLI
gh repo create optimal-sizing --public --source=. --remote=origin --push

# Option B: Git commands
git branch -M main
git remote add origin https://github.com/Xtley001/optimal-sizing.git
git push -u origin main
```

## Continuous Integration & Documentation

- **CI Pipeline:** [`.github/workflows/ci.yml`](./.github/workflows/ci.yml) validates `cargo build`, `cargo test`, `cargo clippy`, and verifies zero external chain dependencies in `sizing-core`.
- **Docs Deployment:** [`.github/workflows/docs.yml`](./.github/workflows/docs.yml) builds `cargo doc` and publishes to GitHub Pages on pushes to `main`.

## Benchmarks

```bash
# Generate wall-clock benchmarks against naive grid search
cargo run -p sizing-bench --release
```
Outputs report to [docs/benchmark-report.md](./docs/benchmark-report.md).
