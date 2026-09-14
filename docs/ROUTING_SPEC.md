# Heterogeneous Multi-Hop Routing & CLI Specification

> **Authority:** This document defines the architecture, data structures, joint optimization algorithms, and CLI syntax for multi-hop route sizing across arbitrary DEX curve families. All types, JSON schemas, CLI arguments, and error transitions are explicitly specified.

---

## 1. Heterogeneous `Leg` Matrix

`sizing-router` supports composing arbitrary liquidity curve types into multi-hop routes.

### 1.1 `Leg` Enum Definition
```rust
use sizing_core::curves::{
    BalancerWeighted, ConcentratedLiquidity, Cpmm, CurveCryptoSwap, DodoPmm, Pmm, StableSwap,
};

/// An individual hop in a multi-hop trading path.
pub enum Leg {
    Cpmm(Cpmm),
    StableSwap(StableSwap),
    Pmm(Pmm),
    ConcentratedLiquidity(ConcentratedLiquidity),
    BalancerWeighted(BalancerWeighted),
    DodoPmm(DodoPmm),
    CurveCryptoSwap(CurveCryptoSwap),
}

impl Leg {
    /// Post-fee output for input amount delta_in through this specific leg.
    pub fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        match self {
            Leg::Cpmm(c) => c.quote(delta_in),
            Leg::StableSwap(s) => s.quote(delta_in),
            Leg::Pmm(p) => p.quote(delta_in),
            Leg::ConcentratedLiquidity(cl) => cl.quote(delta_in),
            Leg::BalancerWeighted(b) => b.quote(delta_in),
            Leg::DodoPmm(d) => d.quote(delta_in),
            Leg::CurveCryptoSwap(cs) => cs.quote(delta_in),
        }
    }
}
```

---

## 2. Multi-Hop `Route` Architecture

### 2.1 Route Data Structure
```rust
/// An N-hop sequential route where the output of leg i is the input of leg i+1.
pub struct Route {
    pub legs: Vec<Leg>, // length >= 2
}
```

### 2.2 Why Joint Route Sizing is Required
Let a route be $R = [L_1, L_2, \ldots, L_N]$.
The composite output function is:
$$\text{Output}_R(x) = L_N\left(L_{N-1}\left(\ldots L_1(x)\right)\right)$$
The net route profit function against whole-route reference price $P$ (units of Leg $N$ output per unit of Leg 1 input) and fixed execution cost $c$ is:
$$\text{Profit}_R(x) = \text{Output}_R(x) - P \cdot x - c$$

> **Crucial Invariant:** Sizing each leg independently against intermediate prices does NOT yield the route-optimal input size. `sizing-router` directly searches for $x^* = \arg\max_{x \ge 0} \text{Profit}_R(x)$.

### 2.3 Joint Golden-Section Search Algorithm
```text
Inputs: route R, reference_price P, constraints SizingConstraints
Output: SizingResult

1. Probe marginal price at epsilon = 1e-6:
     probe_out = R.quote_route(epsilon)
     marginal_rate = probe_out / epsilon
     If marginal_rate <= P -> Return Err(SizingError::NoArbitrageOpportunity)

2. Bracket upper bound:
     hi = 1.0
     While Profit_R(hi * 2) > Profit_R(hi):
       hi = hi * 2
       guard += 1
       If guard > 60 -> break
     hi = hi * 2
     If constraints.max_size is Some(cap) -> hi = min(hi, cap)

3. Run Golden-Section search on Profit_R(x) over [0, hi] with tolerance = 1e-6:
     (optimal_delta, iterations) = golden_section_maximize(Profit_R, 0, hi, 1e-6)

4. Evaluate net expected profit:
     gross_profit = R.quote_route(optimal_delta) - P * optimal_delta
     expected_profit = gross_profit - constraints.fixed_cost
     If expected_profit <= 0 -> Return Err(SizingError::NoProfitableSize)

5. Collect diagnostic guarantee tiers from each leg:
     tiers = [leg_1.tier, ..., leg_N.tier]
     guarantee_tier = GuaranteeTier::ComposedFrom(tiers)

6. Return Ok(SizingResult { optimal_delta, expected_profit, guarantee_tier, iterations })
```

---

## 3. CLI Expansion (`sizing-cli`)

`sizing-cli` is expanded to provide direct terminal access for all new curves and arbitrary route specifications.

### 3.1 Subcommand Syntax Table

| Subcommand | Flag Arguments | Example |
|---|---|---|
| `cpmm` | `--x`, `--y`, `--fee`, `--price`, `[--fixed-cost]`, `[--max-size]` | `sizing-cli cpmm --x 1200000 --y 800000 --fee 0.997 --price 0.60` |
| `stableswap` | `--reserves` (csv), `--amplification`, `--fee`, `--price` | `sizing-cli stableswap --reserves 15000000,8000000,9500000 --amplification 100 --fee 0.997 --price 0.5` |
| `pmm` | `--base-reserve`, `--quote-reserve`, `--oracle-price`, `--k`, `--price` | `sizing-cli pmm --base-reserve 1000000 --quote-reserve 2000000000 --oracle-price 2000 --k 0.0000001 --price 1900` |
| `clmm` | `--liquidity`, `--sqrt-price-lower`, `--sqrt-price-upper`, `--sqrt-price-current`, `--fee`, `--price` | `sizing-cli clmm --liquidity 1000000 --sqrt-price-lower 0.5 --sqrt-price-upper 2.0 --sqrt-price-current 1.0 --fee 0.997 --price 0.9` |
| `balancer` | `--reserves` (csv), `--weights` (csv), `--fee`, `--price` | `sizing-cli balancer --reserves 1000000,2000000 --weights 0.8,0.2 --fee 0.997 --price 0.25` |
| `dodo` | `--base-reserve`, `--quote-reserve`, `--target-base`, `--target-quote`, `--oracle-price`, `--k`, `--price` | `sizing-cli dodo --base-reserve 1000 --quote-reserve 2000000 --target-base 1000 --target-quote 2000000 --oracle-price 2000 --k 0.1 --price 1950` |
| `cryptoswap` | `--reserves` (csv), `--a`, `--gamma`, `--price-scale`, `--fee`, `--price` | `sizing-cli cryptoswap --reserves 1000,2000000 --a 400000 --gamma 0.0001 --price-scale 2000 --fee 0.996 --price 1950` |
| `route` | `--spec-file` (JSON) OR `--legs-json` (string), `--price`, `[--fixed-cost]`, `[--max-size]` | `sizing-cli route --legs-json '[{"type":"cpmm","x":"1000000","y":"2000000","fee":"0.997"},{"type":"balancer","reserves":["2000000","500000"],"weights":["0.5","0.5"],"fee":"0.997"}]' --price 0.24` |

### 3.2 Output Schema
```text
optimal_delta:    63201.877941580268260688898696
expected_profit:  1991.246971608191627795187204
guarantee_tier:   ComposedFrom([ProvenOptimal, ProvenOptimal])
iterations:       38
```
