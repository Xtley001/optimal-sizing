# optimal-sizing (Python)

A chain-agnostic mathematical library for profit-maximizing AMM trade sizing across heterogeneous liquidity curves.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Version](https://img.shields.io/badge/version-2.0.0-green.svg)](https://github.com/Xtley001/optimal-sizing)
[![Python 3.9+](https://img.shields.io/badge/python-3.9+-blue.svg)](https://www.python.org/downloads/)

`optimal-sizing` computes exact profit-maximizing trade sizes for constant-product (Uniswap v2), StableSwap (Curve v1), concentrated liquidity (Uniswap v3), Balancer, Velodrome/Aerodrome, Discretized Liquidity Book (DLMM), Curve CryptoSwap v2, and DODO PMM pools with explicit mathematical guarantee tiers.

## Installation

```bash
pip install optimal-sizing
```

## Quickstart

```python
from decimal import Decimal
from sizing_py import PyCpmm, PyVelodromeStable, portfolio_optimize

# 1. Single pool sizing
cpmm = PyCpmm(Decimal("1000000"), Decimal("1000000"), Decimal("0.997"))
res = cpmm.optimal_size(reference_price=Decimal("0.95"), fixed_cost=Decimal("10"))
print(f"Optimal delta: {res.optimal_delta}, Expected profit: {res.expected_profit} ({res.guarantee_tier})")

# 2. Multi-pool joint portfolio allocation under hard budget constraint
velo = PyVelodromeStable(Decimal("2000000"), Decimal("2000000"), Decimal("0.9995"))
portfolio = portfolio_optimize(
    total_capital_budget=Decimal("50000"),
    opportunities=[
        {"id": "uniswap_v2", "pool": cpmm, "reference_price": Decimal("0.95")},
        {"id": "aerodrome", "pool": velo, "reference_price": Decimal("0.95")},
    ],
)
print(f"Total capital used: {portfolio.total_capital_used}, Profit: {portfolio.total_expected_profit}")
for pos in portfolio.allocations:
    print(f" - {pos.id}: allocated {pos.allocated_size}, profit {pos.expected_profit}")
```

## Supported Curve Families

| Curve Family | Protocols | Solver Strategy | Guarantee Tier |
|---|---|---|---|
| Constant-Product (`x*y=k`) | Uniswap v2, SushiSwap | Closed-form analytical root | `ProvenOptimal` |
| StableSwap | Curve v1, Ellipsis | Newton-Raphson + Golden-Section | `NumericallyGuaranteed` |
| Concentrated Liquidity | Uniswap v3 / v4 | Virtual reserve analytical mapping | `ProvenOptimal` |
| Velodrome Stable (`x³y + xy³ = k`) | Velodrome, Aerodrome | Normalized Newton-Raphson | `NumericallyGuaranteed` |
| Discretized Liquidity Book | Trader Joe, Meteora | Stepwise bin aggregation | `ProvenOptimal` |
| Curve CryptoSwap | Curve v2 | Dynamic invariant root-solver | `NumericallyGuaranteed` |
| Balancer Weighted Pool | Balancer v1 / v2 | Generalized invariant solver | `NumericallyGuaranteed` |
| Proactive Market Maker | WooFi v2, DODO | Golden-Section + Unimodality check | `EmpiricallyValidated` |
| Multi-Pool Portfolio | Cross-protocol | Dual water-filling Lagrange bisection | Budget-feasible |

## License

Released under the [MIT License](https://opensource.org/licenses/MIT). Copyright (c) 2026 Christley OLUBELA (Xtley001).
