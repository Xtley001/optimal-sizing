# Security Policy

## Audit status

`sizing-core` has not been audited. Do not depend on its output for irreversible on-chain actions without your own review of the [`GuaranteeTier`](./docs/API.md) attached to the specific curve you're using — see [`docs/whitepaper.md § 7 Security considerations`](./docs/whitepaper.md#7-security-considerations) for the known attack-vector-to-mitigation table.

## Scope

| Crate | Audit-relevant surface |
|---|---|
| `sizing-core` | The library itself — no I/O, no chain dependency (enforced in CI via `cargo tree`, see [`.github/workflows/ci.yml`](./.github/workflows/ci.yml)). This is the crate to review before depending on for anything financially meaningful. Includes `ConcentratedLiquidity` and a set of correctness fixes to `StableSwap`/`Pmm` found in a later code review — see [`CHANGELOG.md`](./CHANGELOG.md). |
| `sizing-bench` | Benchmark/comparison tooling only. Not part of the public API surface. |
| `sizing-integration` | Reference integration against live Ethereum state. Read-only, sends no transactions — but see the known-risk items below before trusting its printed output. Includes Multicall3 batching — same never-run-live caveat applies. |
| `sizing-router` | Composes two `sizing-core` curves into a single route. A composed result's `GuaranteeTier::ComposedFrom` is deliberately never `ProvenOptimal`-equivalent even when both legs are — treat it like `EmpiricallyValidated` when deciding what to trust. No I/O. |
| `sizing-wasm` | Thin `wasm-bindgen` boundary wrapper, no independent math. Has not been compiled to its actual `wasm32-unknown-unknown` target or run in a browser — only host-target `cargo check` has been done. Do not treat as browser-verified. |
| `sizing-py` | Thin PyO3 boundary wrapper, no independent math. Unlike `sizing-wasm`, this one has been built and run end-to-end against a real Python 3.12 interpreter — see [`CHANGELOG.md`](./CHANGELOG.md) for what was tested. |
| `sizing-cli` | Thin CLI wrapper over `sizing-core`/`sizing-router`. No independent math; inherits whatever guarantee tier the underlying call returns. |

## Known unverified assumptions

`sizing-integration`'s Curve 3pool `A()` scaling has never been checked against live chain state (no Ethereum RPC egress was available while building this). If wrong, every `StableSwap` result out of that integration is off by 100x on the amplification coefficient. See [`docs/ARCHITECTURE.md § Reference integration`](./docs/ARCHITECTURE.md) for the full explanation and the `CURVE_A_OVERRIDE` environment variable that lets you supply a manually verified value instead of trusting the default.

## Reporting a vulnerability

Open a GitHub issue for non-sensitive findings (e.g. a formula discrepancy against the whitepaper, a test gap). For anything sensitive — a correctness bug that could cause a caller to lose funds by trusting a `SizingResult` — contact the maintainer directly rather than filing a public issue, so a fix can land before the report becomes public.

## Supported versions

This project is pre-`1.0.0`. Per semver convention for `0.x` releases, only the latest published `0.x` version is supported; there is no back-porting of fixes to older `0.x` releases.
