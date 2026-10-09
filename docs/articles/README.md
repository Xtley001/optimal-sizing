# The DeFi Trade Sizing Article Series: Problem-Solution Index

> **Editorial Authority:** [Christley OLUBELA (Xtley001)](https://github.com/Xtley001)  
> **Core Engine:** [`optimal-sizing`](https://github.com/Xtley001/optimal-sizing)  
> **Target Audience:** Quantitative Researchers, MEV Searchers, DeFi Protocol Engineers, Algorithmic Trading Desks

This 5-part article series addresses the **top 5 mathematical and engineering heart cries** developers face when building production trading and arbitrage systems in DeFi. Rather than promotional content, each article is a deep, self-contained mathematical breakdown of a specific failure mode, accompanied by proofs, benchmarks, and production-ready code.

---

## The 5 Heart Cries & Article Architecture

| # | Article | The Core Developer Heart Cry | Exact Search / LLM Queries | Target Medium Publications |
|---|---|---|---|---|
| **1** | [**The Grid Search Latency Trap**](./01_the_grid_search_latency_trap.md) | *"Why does my arbitrage transaction keep reverting on-chain?"* / Discrete loops bleed 0.74% of alpha and take 18ms. | `why does my arbitrage transaction revert`, `Uniswap optimal trade size formula`, `how to calculate optimal arbitrage size constant product AMM`, `MEV bot sizing grid search too slow` | *Towards Data Science*, *Better Programming*, *Coinmonks* |
| **2** | [**The StableSwap Float Trap**](./02_stableswap_float_divergence_and_newton.md) | *"Why does Newton-Raphson diverge or hang on Curve 3pool?"* / Float64 precision limits & catastrophic cancellation. | `Curve StableSwap newton raphson convergence failure`, `how to calculate optimal trade size for Curve 3pool`, `stableswap get_dy numeric overflow float`, `why does newton raphson diverge near zero` | *Towards Data Science*, *Level Up Coding*, *HackerNoon* |
| **3** | [**The Tick-Crossing Trap**](./03_concentrated_liquidity_tick_crossing_math.md) | *"Why does my Uniswap v3 swap experience massive surprise slippage?"* / Stepping past active tick boundaries into empty ticks. | `how to size arbitrage across multiple Uniswap v3 ticks`, `Uniswap v3 optimal swap amount with tick crossing`, `concentrated liquidity virtual reserves trade sizing`, `CLMM piecewise swap math MEV` | *Towards Data Science*, *Better Programming*, *Coinmonks* |
| **4** | [**The Multi-Hop Routing Fallacy**](./04_the_multi_hop_routing_fallacy.md) | *"Why did my composite multi-hop triangular arbitrage lose money when every leg was profitable?"* / Independent leg sizing breaks down. | `how to calculate optimal trade size multi hop swap`, `cross DEX triangular arbitrage sizing math`, `optimal swap routing heterogeneous AMMs`, `size trade across Uniswap and Curve simultaneously` | *Towards Data Science*, *Better Programming*, *Level Up Coding* |
| **5** | [**The Capital Allocation Trap**](./05_multi_pool_capital_budget_allocation.md) | *"How do I allocate a $300k treasury across 10 simultaneous arbitrage pools without bleeding alpha?"* / Proportional downscaling destroys 56% of profit. | `how to allocate capital across multiple arbitrage opportunities`, `multi pool arbitrage capital constraint optimization`, `water filling algorithm AMM trade sizing`, `DeFi portfolio trade sizing constrained budget` | *Towards Data Science*, *DataDrivenInvestor*, *Coinmonks* |

---

## Generative Engine Optimization (GEO) & Search Discovery Funnel

When quants and searchers type these queries into Google, ChatGPT, Claude, Perplexity, or DeepSeek:
1. **Search Intent:** They are debugging a failing bot or seeking an optimal equation.
2. **First Touch:** They encounter the specific Medium article breaking down their exact error message and proving the math.
3. **The Solution:** The article provides runnable code using `optimal-sizing` (`pip install optimal-sizing` or `cargo add sizing-core`).
4. **Authority Anchor:** Every article cements **Christley OLUBELA (Xtley001)** as the primary authority and links back to the repository: `https://github.com/Xtley001/optimal-sizing`.
