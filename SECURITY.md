# Security Policy

## Audit Status

`sizing-core` has not been audited by a third-party security firm. Do not depend on its output for irreversible on-chain transactions without verifying the [`GuaranteeTier`](./docs/API.md) of the specific curve. For known attack vectors and formal mitigations, see [docs/whitepaper.md § 7](./docs/whitepaper.md#7-security-considerations).

## Scope

| Crate | Audit Surface | Security Boundary |
|---|---|---|
| `sizing-core` | Core mathematical solvers | Zero I/O, zero chain dependencies, arbitrary-precision Decimal |
| `sizing-router` | Multi-hop composite routes | Cascades quotes deterministically; emits `GuaranteeTier::ComposedFrom` |
| `sizing-portfolio` | Constrained capital allocation | Enforces hard budget feasibility ($\sum c_i \le B$) via dual water-filling |
| `sizing-watch` | Real-time WebSocket daemon | Handles JSON-RPC `newHeads` event streams with polling fallback |
| `sizing-backtest` | Historical replay engine | Replays archived state trajectories against grid-search benchmarks |
| `sizing-cli` | Command-line interface | Thin CLI and JSON route file deserialization |
| `sizing-py` | PyO3 Python bindings | Thin boundary type conversion (`Decimal` to/from Python `decimal.Decimal`) |
| `sizing-wasm` | WebAssembly bindings | Thin `wasm-bindgen` boundary wrapper and TypeScript definitions |
| `sizing-bench` | Benchmark suite | Performance measurement tooling only; excluded from runtime audit |
| `sizing-integration` | Reference Ethereum integration | Read-only RPC multicall verification against live pool state |

## Known Assumptions

- **Amplification Coefficient ($A$):** Curve pools with dynamic $A$-ramping schedules require callers to pass the current effective $A$ at evaluation time.
- **Oracle Staleness:** PMM curves rely on external oracle prices (`oracle_price`). Callers must ensure the oracle value is current to prevent stale pricing arbitrage.
- **Calldata Costs:** Rollup execution gas models use standard EIP-4844 blob base fees. L1 gas spike buffers must be configured by callers.

## Vulnerability Disclosure

Report security vulnerabilities directly to the maintainer via GitHub Security Advisories or private email rather than opening a public issue. Include:
- Affected curve or crate
- Numerical reproduction inputs
- Impact on capital sizing or convergence

## Supported Versions

| Version | Supported |
|---|---|
| `2.0.x` | Yes |
| `< 2.0.0` | No |
