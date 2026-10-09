# Specification 05: SDK Packaging, Multi-Language Bindings & CLI Enhancements

> **Document**: `update/05_PACKAGING_AND_BINDINGS_SPEC.md`  
> **Author**: Xtley001  
> **Crates Target**: `crates/sizing-py`, `crates/sizing-wasm`, `crates/sizing-cli`  
> **Status**: Ready for Implementation  
> **Standards Compliance**: Claude Build Master Skill § 2 & § 3  

---

## 1. Domain & Boundary Definition

### What This Document Owns
- Creation of `pyproject.toml` and Python type stubs (`.pyi`) for `sizing-py` enabling PyPI distribution (`pip install optimal-sizing`).
- Creation of npm `package.json` and TypeScript typings (`index.d.ts`) for `sizing-wasm` enabling `@optimal-sizing/wasm` distribution.
- Command-line interface enhancement in `sizing-cli` allowing multi-hop route configuration via JSON files.

### What This Document Does NOT Own
- Sizing core mathematical routines (delegates to `sizing-core` and `sizing-router`).

---

## 2. Python Package Configuration (`sizing-py`)

File path: `crates/sizing-py/`

### 2.1 `pyproject.toml` Specification
Create `crates/sizing-py/pyproject.toml`:

```toml
[build-system]
requires = ["maturin>=1.5,<2.0"]
build-backend = "maturin"

[project]
name = "optimal-sizing"
version = "0.1.0"
description = "Chain-agnostic profit-maximizing AMM trade sizing in Rust with Python bindings"
authors = [{ name = "Xtley001", email = "xtley001@users.noreply.github.com" }]
license = { text = "MIT" }
readme = "README.md"
requires-python = ">=3.9"
keywords = ["defi", "amm", "arbitrage", "mev", "uniswap", "curve", "balancer", "trading", "quant"]
classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Financial and Insurance Industry",
    "License :: OSI Approved :: MIT License",
    "Programming Language :: Rust",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.9",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Topic :: Office/Business :: Financial :: Investment",
]

[tool.maturin]
features = ["pyo3/extension-module"]
module-name = "sizing_py"
python-source = "python"
```

### 2.2 Python Type Stubs (`sizing_py.pyi`)
Create `crates/sizing-py/python/sizing_py/__init__.pyi`:

```python
from decimal import Decimal
from typing import Optional, List, Any

class PySizingConstraints:
    fixed_cost: Decimal
    max_size: Optional[Decimal]
    def __init__(self, fixed_cost: Decimal = Decimal(0), max_size: Optional[Decimal] = None) -> None: ...

class SizingResult:
    optimal_delta: Decimal
    expected_profit: Decimal
    guarantee_tier: str
    guarantee_tier_detail: Any
    iterations: Optional[int]

class PyCpmm:
    def __init__(self, x: Decimal, y: Decimal, fee_retention: Decimal) -> None: ...
    def quote(self, delta_in: Decimal) -> Decimal: ...
    def optimal_size(self, reference_price: Decimal, constraints: PySizingConstraints) -> SizingResult: ...

class PyStableSwap:
    def __init__(self, reserves: List[Decimal], amplification: Decimal, fee_retention: Decimal) -> None: ...
    def quote(self, delta_in: Decimal) -> Decimal: ...
    def optimal_size(self, reference_price: Decimal, constraints: PySizingConstraints) -> SizingResult: ...

class PyPmm:
    def __init__(self, base_reserve: Decimal, quote_reserve: Decimal, oracle_price: Decimal, k: Decimal) -> None: ...
    def quote(self, delta_in: Decimal) -> Decimal: ...
    def optimal_size(self, reference_price: Decimal, constraints: PySizingConstraints) -> SizingResult: ...

class PyConcentratedLiquidity:
    def __init__(self, liquidity: Decimal, sqrt_price_lower: Decimal, sqrt_price_upper: Decimal, sqrt_price_current: Decimal, fee_retention: Decimal) -> None: ...
    def quote(self, delta_in: Decimal) -> Decimal: ...
    def optimal_size(self, reference_price: Decimal, constraints: PySizingConstraints) -> SizingResult: ...

class PyBalancerWeightedPool:
    def __init__(self, reserves: List[Decimal], weights: List[Decimal], fee_retention: Decimal, input_index: int = 0, output_index: int = 1) -> None: ...
    def quote(self, delta_in: Decimal) -> Decimal: ...
    def optimal_size(self, reference_price: Decimal, constraints: PySizingConstraints) -> SizingResult: ...
```

---

## 3. WASM & NPM Package Configuration (`sizing-wasm`)

File path: `crates/sizing-wasm/`

### 3.1 `package.json` Specification
Create `crates/sizing-wasm/package.json`:

```json
{
  "name": "@optimal-sizing/wasm",
  "version": "0.1.0",
  "description": "Chain-agnostic profit-maximizing AMM trade sizing compiled to WebAssembly",
  "author": "Xtley001",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/Xtley001/optimal-sizing.git"
  },
  "main": "sizing_wasm.js",
  "types": "sizing_wasm.d.ts",
  "files": [
    "sizing_wasm_bg.wasm",
    "sizing_wasm.js",
    "sizing_wasm.d.ts"
  ],
  "keywords": [
    "defi",
    "amm",
    "arbitrage",
    "mev",
    "webassembly",
    "uniswap",
    "curve"
  ]
}
```

---

## 4. Enhanced Multi-Hop Route Loading in `sizing-cli`

File path: `crates/sizing-cli/src/main.rs`

### 4.1 Subcommand Specification
Add new CLI subcommand:

```bash
sizing-cli route-file --file path/to/route.json --price 0.9985 --fixed-cost 15.00
```

### 4.2 JSON Route Schema Example (`route.json`)

```json
{
  "legs": [
    {
      "type": "cpmm",
      "x": "1200000",
      "y": "800000",
      "fee": "0.997"
    },
    {
      "type": "stableswap",
      "reserves": ["15000000", "8000000", "9500000"],
      "amplification": "100",
      "fee": "0.9996"
    }
  ]
}
```

---

## 5. Acceptance Criteria & Test Contract

1. **Python Packaging**:
   - `maturin build --release` produces valid `.whl` wheel in `target/wheels/`.
   - `pip install` into clean virtual environment succeeds and verifies `import sizing_py`.
2. **WASM Packaging**:
   - `wasm-pack build --target nodejs` in `crates/sizing-wasm` succeeds and passes node import check.
3. **CLI Route Execution**:
   - `cargo run -p sizing-cli -- route-file --file tests/fixtures/route_cpmm_stableswap.json --price 0.9985 --fixed-cost 10.0`
   - Prints optimal trade size with `guarantee_tier: ComposedFrom`.
