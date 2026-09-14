# Error Catalog: `optimal-sizing`

> **Authority:** This document is the comprehensive catalog of every error state in `optimal-sizing`. Every error names its trigger, enum variant/type, CLI exit code, and exact recovery path.

---

## 1. `SizingError` Enum Catalog (`sizing-core`)

Every fallible calculation in `sizing-core` and `sizing-router` returns `Result<T, SizingError>`.

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SizingError {
    InvalidReserves { detail: String },
    DidNotConverge { iterations_attempted: u32 },
    NoProfitableSize,
    NoArbitrageOpportunity,
}
```

---

### 1.1 `SizingError::InvalidReserves`
* **Trigger:**
  - Any reserve $\le 0$.
  - StableSwap amplification coefficient $A < 1$.
  - Fee retention $f \le 0$ or $f > 1$.
  - Reference price $P \le 0$.
  - Tick bounds inverted ($\sqrt{p_{upper}} \le \sqrt{p_{lower}}$) or current price out of tick range.
  - Multi-tick trade exceeds outer bounds of initialized tick arrays.
  - Balancer weights do not sum to $1.0$ or any weight $\le 0$.
  - Gas parameters ($gas\_price$, $gas\_units$) negative.
* **Rust Error Shape:** `SizingError::InvalidReserves { detail: "<explanation>" }`
* **Python Mapping (`sizing-py`):** Raises `ValueError("invalid reserves: <detail>")`.
* **WASM Mapping (`sizing-wasm`):** `WasmSizingResult { ok: false, error: "InvalidReserves: <detail>", ... }`.
* **CLI Exit:** Prints error message to `stderr`, exits with status code `1`.
* **Recovery Path:** Validate input pool state from RPC/node before constructing curve struct.

---

### 1.2 `SizingError::DidNotConverge`
* **Trigger:**
  - Newton's method in `StableSwap::solve_d` or `solve_new_output_reserve` exceeds `MAX_NEWTON_ITERATIONS` (255) without reaching `NEWTON_TOLERANCE` ($10^{-6}$).
  - Curve CryptoSwap 2D Newton iteration fails to converge within 255 steps.
* **Rust Error Shape:** `SizingError::DidNotConverge { iterations_attempted: u32 }`
* **Python Mapping:** Raises `ValueError("solver failed to converge after <n> iterations")`.
* **WASM Mapping:** `WasmSizingResult { ok: false, error: "DidNotConverge", ... }`.
* **CLI Exit:** Exits with status code `1`.
* **Recovery Path:** Verify that the pool reserves are not severely degenerate or imbalanced by more than 10 orders of magnitude.

---

### 1.3 `SizingError::NoProfitableSize`
* **Trigger:**
  - A mispricing exists ($marginal\_price > P$), but after netting out `constraints.fixed_cost`, the maximum attainable profit is $\le 0$.
  - `constraints.max_size` is so small that fixed execution costs cannot be amortized.
* **Rust Error Shape:** `SizingError::NoProfitableSize`
* **Python Mapping:** Raises `ValueError("NoProfitableSize: trade cannot clear fixed cost threshold")`.
* **WASM Mapping:** `WasmSizingResult { ok: false, error: "NoProfitableSize", ... }`.
* **CLI Exit:** Exits with status code `0` (clean non-execution) or code `1` if strict mode enabled.
* **Recovery Path:** Do not submit an on-chain transaction. The opportunity is unprofitable after gas costs.

---

### 1.4 `SizingError::NoArbitrageOpportunity`
* **Trigger:**
  - Pool marginal price at zero size ($marginal\_price \le P$) does not beat the external reference price.
  - The pool is fairly priced or mispriced in the opposite direction.
* **Rust Error Shape:** `SizingError::NoArbitrageOpportunity`
* **Python Mapping:** Raises `ValueError("NoArbitrageOpportunity: pool price is not mispriced relative to reference price")`.
* **WASM Mapping:** `WasmSizingResult { ok: false, error: "NoArbitrageOpportunity", ... }`.
* **CLI Exit:** Exits with status code `0` or prints "No arbitrage opportunity found."
* **Recovery Path:** Normal operational state. No trade needed. Check reverse trading direction if applicable.

---

## 2. Streaming & Integration Operational Errors (`sizing-watch` / `sizing-integration`)

| Error Code | Trigger | Daemon Behavior | Log Level | Recovery Path |
|---|---|---|---|---|
| `ERR_RPC_UNREACHABLE` | Node HTTP/WS connection dropped | Exponential backoff reconnect (1s, 2s, 4s... max 30s) | `ERROR` | Automatic reconnect; fallback to backup RPC endpoint if configured |
| `ERR_STALE_BLOCK` | Block timestamp $> 60\text{s}$ old | Flag pool cache as `Stale`; suppress sizing outputs | `WARN` | Resync node block headers |
| `ERR_REORG_DETECTED` | Parent hash mismatch on new block | Evict pool cache; query Multicall3 batch fresh state | `WARN` | Automatic state refresh via Multicall3 |
| `ERR_ORACLE_FEED_SILENT`| No price updates for $> 10\text{s}$ | Halt automated alerts for affected pair | `WARN` | Reconnect oracle WebSocket feed |
