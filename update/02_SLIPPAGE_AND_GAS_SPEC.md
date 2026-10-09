# Specification 02: Adversarial Slippage & Rollup Gas Engine

> **Document**: `update/02_SLIPPAGE_AND_GAS_SPEC.md`  
> **Author**: Xtley001  
> **Crate Target**: `sizing-core` (`slippage.rs`, `gas.rs`)  
> **Status**: Ready for Implementation  
> **Standards Compliance**: Claude Build Master Skill § 2 & § 3  

---

## 1. Domain & Boundary Definition

### What This Document Owns
- Mathematical formulation of curve-exact adversarial reserve displacement (sandwich / frontrunning modeling).
- Implementation of `AdversePricingCurve` trait and `CurveSlippageBound` type.
- Empirical and calibrated protocol gas constants for major EVM AMMs (Uniswap, Curve, Balancer, Aerodrome).
- L2 Rollup fee modeling incorporating EIP-4844 blobs (Optimism Bedrock/Ecotone, Arbitrum Nitro, Base).

### What This Document Does NOT Own
- Live Ethereum or L2 RPC querying (parameters like `gas_price_wei` or `blob_fee_wei` are caller supplied).
- AMM invariant execution logic (delegates to the underlying curve).

---

## 2. Curve-Exact Adversarial Slippage Bounds

### 2.1 Problem Formulation
`SlippageBound::from_quote` applies a linear discount:
$$\text{min\_amount\_out} = \text{quoted\_output} \cdot (1 - \text{staleness\_fraction})$$

While simple, this ignores curve convexity:
1. If an adversarial or competing transaction swaps $\Delta x_{\text{adv}}$ ahead of our transaction in the block, the pool reserves shift to $(x_{\text{displaced}}, y_{\text{displaced}})$.
2. Our trade of size $\Delta x^*$ executes against the displaced pool, experiencing non-linear price degradation.
3. Submitting an overly loose `min_amount_out` invites MEV sandwich bots; submitting an overly tight `min_amount_out` causes transaction reverts.

### 2.2 The `AdversePricingCurve` Trait
Defined in `crates/sizing-core/src/slippage.rs`:

```rust
use rust_decimal::Decimal;
use crate::error::SizingError;
use crate::traits::PricingCurve;

/// A curve that supports calculating output under adversarial pre-swap frontrunning.
pub trait AdversePricingCurve: PricingCurve {
    /// Computes output of trade `delta_in` assuming competing trades totaling
    /// `adverse_in` execute against the pool immediately prior.
    fn quote_adverse(
        &self,
        delta_in: Decimal,
        adverse_in: Decimal,
    ) -> Result<Decimal, SizingError>;
}
```

### 2.3 Closed-Form CPMM Adverse Calculation
For a Constant-Product pool $(x, y, f)$, if an adverse swap $\Delta x_{\text{adv}}$ precedes our trade $\Delta x$:
1. Adverse swap updates reserves:
   $$x_1 = x + f \cdot \Delta x_{\text{adv}}$$
   $$y_1 = y - \frac{y \cdot f \cdot \Delta x_{\text{adv}}}{x + f \cdot \Delta x_{\text{adv}}} = \frac{x \cdot y}{x + f \cdot \Delta x_{\text{adv}}}$$
2. Our trade executes against $(x_1, y_1)$:
   $$\text{quote}_{\text{adverse}}(\Delta x, \Delta x_{\text{adv}}) = \frac{y_1 \cdot f \cdot \Delta x}{x_1 + f \cdot \Delta x} = \frac{x \cdot y \cdot f \cdot \Delta x}{(x + f \cdot \Delta x_{\text{adv}}) \cdot (x + f \cdot \Delta x_{\text{adv}} + f \cdot \Delta x)}$$

This is solved in **exact closed form in $\mathcal{O}(1)$**.

### 2.4 Concrete Struct Interface

```rust
/// Worst-case minimum output bound evaluated directly against displaced pool reserves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurveSlippageBound {
    /// Exact minimum amount out under adverse displacement.
    pub min_amount_out: Decimal,
    /// The adversarial input volume assumed for this bound.
    pub adverse_delta_assumed: Decimal,
    /// Effective realized slippage percentage: (unadverse_quote - min_amount_out) / unadverse_quote.
    pub effective_slippage_fraction: Decimal,
}

impl CurveSlippageBound {
    /// Constructs an exact slippage bound from a curve implementing AdversePricingCurve.
    pub fn from_curve<C: AdversePricingCurve>(
        curve: &C,
        delta_in: Decimal,
        adverse_in: Decimal,
    ) -> Result<Self, SizingError>;
}
```

---

## 3. Calibrated Protocol & Rollup Gas Engine

File path: `crates/sizing-core/src/gas.rs`

### 3.1 Empirical Gas Constants
Standard EVM gas units consumed per swap on verified contracts:

```rust
/// Standard verified EVM execution gas units consumed per single-hop swap.
pub struct ProtocolGasConstants;

impl ProtocolGasConstants {
    /// Uniswap v2 swap: ~105,000 gas (including router overhead and transfer).
    pub const UNISWAP_V2_SWAP: u64 = 105_000;

    /// Uniswap v3 single-tick swap: ~130,000 gas.
    pub const UNISWAP_V3_SWAP: u64 = 130_000;

    /// Curve Finance 3pool swap: ~190,000 gas.
    pub const CURVE_3POOL_SWAP: u64 = 190_000;

    /// Balancer v2 weighted pool swap: ~140,000 gas.
    pub const BALANCER_V2_SWAP: u64 = 140_000;

    /// Aerodrome / Velodrome stable swap: ~125,000 gas.
    pub const AERODROME_STABLE_SWAP: u64 = 125_000;

    /// DODO PMM swap: ~150,000 gas.
    pub const DODO_PMM_SWAP: u64 = 150_000;
}
```

### 3.2 L2 Rollup Fee Model (EIP-4844 Blob Gas Accounting)
On Ethereum Layer 2 rollups (Optimism, Base, Arbitrum), transaction fee consists of:
1. **L2 Execution Fee**: $\text{GasUsed}_{\text{L2}} \times \text{L2GasPrice}$
2. **L1 Data / Blob Fee**: Fee paid to post transaction calldata to Ethereum L1 via EIP-4844 blobs.

#### Formula for Optimism / Base (Ecotone Spec)
$$\text{L1Fee} = \text{calldata\_bytes} \times \left(16 \cdot \text{L1BaseFee} \cdot \frac{\text{baseFeeScalar}}{10^6} + \text{L1BlobBaseFee} \cdot \frac{\text{blobBaseFeeScalar}}{10^6}\right)$$

#### Rust Interface

```rust
/// Parameters for calculating L2 transaction costs including L1 blob overhead.
#[derive(Debug, Clone, Copy)]
pub struct L2RollupGasConfig {
    /// Execution gas units on the L2.
    pub l2_execution_gas: u64,
    /// Current L2 gas price in wei.
    pub l2_gas_price_wei: Decimal,
    /// Estimated calldata size in bytes (typically ~160 bytes for an AMM swap).
    pub calldata_bytes: u64,
    /// Ethereum L1 base fee in wei.
    pub l1_base_fee_wei: Decimal,
    /// Ethereum L1 blob base fee in wei (EIP-4844).
    pub l1_blob_base_fee_wei: Decimal,
    /// Rollup base fee scalar (e.g. from OP SystemConfig).
    pub base_fee_scalar: u32,
    /// Rollup blob fee scalar (e.g. from OP SystemConfig).
    pub blob_base_fee_scalar: u32,
}

/// Calculates total fixed transaction cost in output asset units for an L2 transaction.
pub fn l2_rollup_fixed_cost(
    config: L2RollupGasConfig,
    native_token_price_in_output_units: Decimal,
) -> Result<Decimal, SizingError>;
```

---

## 4. Acceptance Criteria & Test Contract

1. **Adverse Slippage Tests**:
   - `cpmm_adverse_slippage_matches_displaced_pool_exact`: Verify that `cpmm.quote_adverse(delta, adverse)` equals `displaced_cpmm.quote(delta)`.
   - `adverse_slippage_strictly_greater_than_normal`: Output under positive adverse flow is strictly less than normal output.
2. **L2 Gas Engine Tests**:
   - `l2_fixed_cost_zero_blob_fee_matches_legacy`: Verify correct fallback when blob fee is zero.
   - `l2_fixed_cost_calculates_exact_output_denomination`: Verify conversion from wei to token units with decimal scaling.
3. **Execution**:
   ```bash
   cargo test -p sizing-core --test slippage_proptest --test gas_direct_tests
   ```
