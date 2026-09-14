# Mathematical & Algorithmic Curve Specifications (v0.2.0 - v0.3.0)

> **Authority:** This document defines the exact mathematical derivations, invariants, quoting functions, optimal sizing algorithms, and `GuaranteeTier` classifications for all curve families in `sizing-core`. All formulas, variable names, and constraints are exact and unambiguous.

---

## 1. Multi-Tick Concentrated Liquidity (Uniswap v3 / v4)

### 1.1 Mathematical Model & Virtual Reserves
A concentrated liquidity pool consists of active liquidity $L$ distributed across discrete price ticks $i \in \mathbb{Z}$, where tick $i$ corresponds to price $p(i) = 1.0001^i$ and $\sqrt{p(i)} = 1.0001^{i/2}$.

Within any single contiguous tick interval $[p_l, p_u]$ with constant active liquidity $L$, the pool obeys the virtual constant-product invariant:
$$\left(x + \frac{L}{\sqrt{p_u}}\right) \cdot \left(y + L\sqrt{p_l}\right) = L^2$$

Defining virtual reserves:
$$x_{virt} = \frac{L}{\sqrt{p_{curr}}}, \quad y_{virt} = L \cdot \sqrt{p_{curr}}$$

### 1.2 Piecewise Quoting Across Multiple Ticks
When a trade $\Delta x$ is executed, the price moves from $\sqrt{p_0}$ downwards toward $\sqrt{p_l}$.
If $\Delta x > \Delta x_{boundary}$ where:
$$\Delta x_{boundary} = \frac{\frac{L}{\sqrt{p_l}} - \frac{L}{\sqrt{p_0}}}{f}$$
the trade exhausts the current tick. The remaining input:
$$\Delta x_{rem} = \Delta x - \Delta x_{boundary}$$
crosses into the next lower tick $[p_{l-1}, p_l]$, where liquidity transitions by net liquidity delta:
$$L_{next} = L_{curr} - \Delta L_{net}(p_l)$$

#### Algorithm: Exact Multi-Tick `quote(delta_in)`
```text
Inputs: delta_in, current_sqrt_price, tick_ranges, fee_retention f
Output: total_delta_out

1. Let remaining_in = delta_in
2. Let current_p = current_sqrt_price
3. Let total_out = 0
4. Find active tick range k where current_p in [p_lower_k, p_upper_k]
5. While remaining_in > 0:
   a. If k is out of initialized range bounds -> Return Err(SizingError::InvalidReserves)
   b. Let L = tick_ranges[k].liquidity_gross
   c. Let max_in_this_tick = (L / tick_ranges[k].lower_sqrt_price - L / current_p) / f
   d. If remaining_in <= max_in_this_tick:
        Let next_p = L / (L / current_p + f * remaining_in)
        Let out_piece = L * (current_p - next_p)
        total_out += out_piece
        remaining_in = 0
      Else:
        Let out_piece = L * (current_p - tick_ranges[k].lower_sqrt_price)
        total_out += out_piece
        remaining_in -= max_in_this_tick
        current_p = tick_ranges[k].lower_sqrt_price
        k = k - 1 // Transition to next lower tick
6. Return Ok(total_out)
```

### 1.3 Optimal Sizing Algorithm
Because virtual constant-product curves are strictly concave within each tick, the composite multi-tick profit function is a contiguous piecewise concave function.

1. **Piecewise Closed-Form Solver:** For each tick $k$, compute candidate unconstrained optimum:
   $$\Delta x^*_k = \frac{\sqrt{x_{virt, k} \cdot y_{virt, k} \cdot f / P} - x_{virt, k}}{f}$$
2. If $\Delta x^*_k$ lies strictly inside tick $k$'s capacity interval, it is the unique global maximum.
3. If marginal price at the lower boundary exceeds $P$, the optimum lies in a lower tick.
4. **Guarantee Tier:** `GuaranteeTier::ProvenOptimal` (proved by concavity of each piecewise segment and strict monotonicity of marginal price).

---

## 2. Balancer Weighted Pools ($V = \prod_{i=1}^n B_i^{w_i}$)

### 2.1 Invariant & Parameterization
For an $n$-asset pool with normalized weights $\sum_{i=1}^n w_i = 1$, the value function invariant is:
$$V = \prod_{i=1}^n B_i^{w_i} = \text{constant}$$

For swapping token $i$ (input reserve $B_i$, weight $w_i$) for token $o$ (output reserve $B_o$, weight $w_o$) with fee retention $f$:
$$B_o' = B_o \cdot \left(\frac{B_i}{B_i + f \cdot \Delta x}\right)^{\frac{w_i}{w_o}}$$
$$\text{quote}(\Delta x) = B_o \cdot \left[1 - \left(\frac{B_i}{B_i + f \cdot \Delta x}\right)^{\frac{w_i}{w_o}}\right]$$

### 2.2 Strict Concavity Proof
The profit function against reference price $P$ is:
$$\text{Profit}(\Delta x) = B_o \cdot \left[1 - \left(\frac{B_i}{B_i + f \cdot \Delta x}\right)^{\frac{w_i}{w_o}}\right] - P \cdot \Delta x - c$$

First derivative:
$$\text{Profit}'(\Delta x) = B_o \cdot \frac{w_i}{w_o} \cdot f \cdot B_i^{\frac{w_i}{w_o}} \cdot (B_i + f \cdot \Delta x)^{-\left(\frac{w_i}{w_o} + 1\right)} - P$$

Second derivative:
$$\text{Profit}''(\Delta x) = -B_o \cdot \frac{w_i}{w_o} \cdot \left(\frac{w_i}{w_o} + 1\right) \cdot f^2 \cdot B_i^{\frac{w_i}{w_o}} \cdot (B_i + f \cdot \Delta x)^{-\left(\frac{w_i}{w_o} + 2\right)}$$

Since $B_i, B_o, w_i, w_o, f > 0$, $\text{Profit}''(\Delta x) < 0$ strictly for all $\Delta x \ge 0$. The profit function is strictly concave everywhere on its domain.

### 2.3 Exact Closed-Form Sizing Formula
Setting $\text{Profit}'(\Delta x) = 0$:
$$(B_i + f \cdot \Delta x)^{\frac{w_i + w_o}{w_o}} = \frac{w_i}{w_o} \cdot \frac{B_o \cdot f \cdot B_i^{\frac{w_i}{w_o}}}{P}$$
$$B_i + f \cdot \Delta x = \left(\frac{w_i \cdot B_o \cdot f}{w_o \cdot P}\right)^{\frac{w_o}{w_i + w_o}} \cdot B_i^{\frac{w_i}{w_i + w_o}} = B_i \cdot \left(\frac{w_i \cdot B_o \cdot f}{w_o \cdot B_i \cdot P}\right)^{\frac{w_o}{w_i + w_o}}$$
$$\boxed{\Delta x^* = \frac{B_i \cdot \left[\left(\frac{w_i \cdot B_o \cdot f}{w_o \cdot B_i \cdot P}\right)^{\frac{w_o}{w_i + w_o}} - 1\right]}{f}}$$

- **Existence Condition:** $\frac{w_i}{w_o} \cdot \frac{B_o \cdot f}{B_i} > P$ (marginal price at $\Delta x = 0$ must exceed $P$).
- **Guarantee Tier:** `GuaranteeTier::ProvenOptimal`.

---

## 3. DODO Proactive Market Maker (PMM)

### 3.1 Mathematical Model
DODO parameterizes liquidity via base target $B_0$, quote target $Q_0$, current reserves $(B, Q)$, oracle guide price $i$, and slippage coefficient $k \in (0, 1]$.

#### Pricing Regimes:
1. **Regime 1: $B < B_0$ (Base Shortage / $R < 1$):**
   $$Q = Q_0 + i \cdot (B_0 - B) \cdot \left(1 + k \cdot \frac{B_0 - B}{B_0}\right)$$
   Marginal price for buying base:
   $$p(B) = i \cdot \left(1 + 2k \cdot \frac{B_0 - B}{B_0}\right)$$

2. **Regime 2: $Q < Q_0$ (Quote Shortage / $R > 1$):**
   $$B = B_0 + \frac{Q_0 - Q}{i} \cdot \left(1 + k \cdot \frac{Q_0 - Q}{Q_0}\right)$$

### 3.2 Sizing Algorithm
Because DODO curves transition quadratically between shortage regimes and asymptotic boundaries, sizing is performed via:
1. **Unimodality Check:** Sample discrete second differences of `quote()` across $[0, \Delta x_{max}]$.
2. **Golden-Section Search:** Maximize $\text{Profit}(\Delta x)$ on $[0, \text{search\_upper\_bound}]$.
3. **Guarantee Tier:** `GuaranteeTier::EmpiricallyValidated { unimodality_confirmed }`.

---

## 4. Curve CryptoSwap (v2 Dynamic Invariant)

### 4.1 Invariant Equation
For 2-asset volatile pairs, Curve v2 defines invariant $D$ governed by amplification parameter $A$ and smoothing parameter $\gamma$:
$$K \cdot D \cdot \gamma + D = A \cdot \gamma \cdot K_0 \cdot \sum x_i + \frac{D^3}{4 \cdot x_0 \cdot x_1}$$
where:
$$K_0 = \frac{4 \cdot x_0 \cdot x_1}{D^2}, \quad K = A \cdot K_0 \cdot \left(\frac{\gamma}{\gamma + 1 - K_0}\right)^2$$

### 4.2 Numerical Solution & Sizing
1. **Invariant $D$:** Solved via multi-variable Newton iteration starting from geometric mean initial guess.
2. **Swap Quoting ($get\_y$):** Given input reserve $x_0 + f \cdot \Delta x$, solve for $x_1$ such that invariant equation equals 0.
3. **Optimal Sizing:** Bisection on finite-difference derivative of profit function, step scaled to $\sqrt{\text{NEWTON\_TOLERANCE}}$.
4. **Guarantee Tier:** `GuaranteeTier::NumericallyGuaranteed { convergence_conditions_met }`.
