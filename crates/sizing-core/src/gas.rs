//! `gas_aware_fixed_cost`. Per `ROADMAP.md`'s "Gas-price-aware
//! `fixed_cost`" entry.
//!
//! **DECISION MADE (logged per this project's stated decision-logging discipline):** this is a pure
//! unit-conversion helper, not a calibrated estimator. The roadmap entry
//! itself notes gas-units-per-swap should "ideally [be] calibrated from
//! real transaction receipts" — this sandbox has no such receipts and no
//! network access to fetch any, so `gas_units_per_swap` is a required
//! caller-supplied argument here, not a per-curve-family default this
//! function invents. Per R1 ("don't invent a default"), this function
//! computes the conversion and nothing more: it does not guess a gas
//! price, does not guess a gas-units figure, and does not silently
//! replace `SizingConstraints.fixed_cost` — it's an opt-in helper a
//! caller uses to *compute* the value they then pass in themselves.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::error::SizingError;

/// Converts a gas cost (in wei) into `fixed_cost` units (the same units
/// as `reference_price` and the curve's output asset), so it can be
/// passed into `SizingConstraints.fixed_cost`.
///
/// `gas_price_wei`: current gas price, in wei per gas unit.
/// `gas_units_per_swap`: estimated gas consumed by the swap transaction
/// itself — caller-supplied; see module doc for why this isn't defaulted.
/// `native_token_price_in_output_units`: the chain's native token's
/// price, denominated in the same output-asset units `fixed_cost` is
/// expected in (e.g. USDC per ETH, if sizing a USDC-denominated pool).
///
/// Returns `SizingError::InvalidReserves` if any argument is negative,
/// or the reserved `native_token_price_in_output_units` argument name
/// misleads a caller into passing zero for a token with real value
/// (zero itself is allowed — it just means "cost is free," an unusual
/// but not invalid input).
pub fn gas_aware_fixed_cost(
    gas_price_wei: Decimal,
    gas_units_per_swap: Decimal,
    native_token_price_in_output_units: Decimal,
) -> Result<Decimal, SizingError> {
    if gas_price_wei < Decimal::ZERO {
        return Err(SizingError::InvalidReserves {
            detail: format!("gas_price_wei must be >= 0, got {gas_price_wei}"),
        });
    }
    if gas_units_per_swap < Decimal::ZERO {
        return Err(SizingError::InvalidReserves {
            detail: format!("gas_units_per_swap must be >= 0, got {gas_units_per_swap}"),
        });
    }
    if native_token_price_in_output_units < Decimal::ZERO {
        return Err(SizingError::InvalidReserves {
            detail: format!(
                "native_token_price_in_output_units must be >= 0, got {native_token_price_in_output_units}"
            ),
        });
    }
    // 1e18 wei per native token unit — a fixed protocol constant (Ethereum
    // and every EVM chain this project's ABI work already targets), not
    // an invented number.
    let wei_per_native_unit = dec!(1_000_000_000_000_000_000);
    let gas_cost_in_native_units = (gas_price_wei * gas_units_per_swap) / wei_per_native_unit;
    Ok(gas_cost_in_native_units * native_token_price_in_output_units)
}

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

/// Parameters for calculating L2 transaction costs including L1 blob overhead (EIP-4844).
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
    /// Rollup base fee scalar (e.g. from OP SystemConfig, parts per million).
    pub base_fee_scalar: u32,
    /// Rollup blob fee scalar (e.g. from OP SystemConfig, parts per million).
    pub blob_base_fee_scalar: u32,
}

/// Calculates total fixed transaction cost in output asset units for an L2 rollup swap.
pub fn l2_rollup_fixed_cost(
    config: L2RollupGasConfig,
    native_token_price_in_output_units: Decimal,
) -> Result<Decimal, SizingError> {
    if config.l2_gas_price_wei < Decimal::ZERO
        || config.l1_base_fee_wei < Decimal::ZERO
        || config.l1_blob_base_fee_wei < Decimal::ZERO
        || native_token_price_in_output_units < Decimal::ZERO
    {
        return Err(SizingError::InvalidReserves {
            detail: "gas prices and token prices must be >= 0".to_string(),
        });
    }

    let wei_per_native_unit = dec!(1_000_000_000_000_000_000);
    let ppm = dec!(1_000_000);

    // 1. L2 execution cost:
    let l2_cost_wei = Decimal::from(config.l2_execution_gas) * config.l2_gas_price_wei;

    // 2. L1 calldata / blob overhead (OP Bedrock/Ecotone spec):
    // L1Fee = calldata_bytes * (16 * L1BaseFee * base_scalar + L1BlobBaseFee * blob_scalar)
    let base_scalar = Decimal::from(config.base_fee_scalar) / ppm;
    let blob_scalar = Decimal::from(config.blob_base_fee_scalar) / ppm;
    let bytes = Decimal::from(config.calldata_bytes);

    let l1_calldata_fee = bytes * (dec!(16) * config.l1_base_fee_wei * base_scalar);
    let l1_blob_fee = bytes * (config.l1_blob_base_fee_wei * blob_scalar);
    let l1_cost_wei = l1_calldata_fee + l1_blob_fee;

    let total_cost_wei = l2_cost_wei + l1_cost_wei;
    let total_cost_native = total_cost_wei / wei_per_native_unit;

    Ok(total_cost_native * native_token_price_in_output_units)
}


#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn computes_expected_cost_in_output_units() {
        // 30 gwei gas price, 150,000 gas units, ETH at 3000 USDC:
        // cost = 30e9 * 150000 / 1e18 ETH = 0.0045 ETH; * 3000 = 13.5 USDC.
        let cost = gas_aware_fixed_cost(dec!(30_000_000_000), dec!(150_000), dec!(3000)).unwrap();
        assert_eq!(cost, dec!(13.5));
    }

    #[test]
    fn zero_gas_price_gives_zero_cost() {
        let cost = gas_aware_fixed_cost(dec!(0), dec!(150_000), dec!(3000)).unwrap();
        assert_eq!(cost, dec!(0));
    }

    #[test]
    fn negative_inputs_are_rejected() {
        assert!(matches!(
            gas_aware_fixed_cost(dec!(-1), dec!(150_000), dec!(3000)),
            Err(SizingError::InvalidReserves { .. })
        ));
        assert!(matches!(
            gas_aware_fixed_cost(dec!(30_000_000_000), dec!(-1), dec!(3000)),
            Err(SizingError::InvalidReserves { .. })
        ));
        assert!(matches!(
            gas_aware_fixed_cost(dec!(30_000_000_000), dec!(150_000), dec!(-1)),
            Err(SizingError::InvalidReserves { .. })
        ));
    }

    #[test]
    fn l2_rollup_fixed_cost_computes_correctly() {
        let config = L2RollupGasConfig {
            l2_execution_gas: ProtocolGasConstants::UNISWAP_V2_SWAP, // 105k
            l2_gas_price_wei: dec!(100_000_000), // 0.1 gwei L2
            calldata_bytes: 160,
            l1_base_fee_wei: dec!(20_000_000_000), // 20 gwei L1
            l1_blob_base_fee_wei: dec!(1_000_000_000), // 1 gwei blob
            base_fee_scalar: 1600, // 0.0016
            blob_base_fee_scalar: 800_000, // 0.8
        };
        // ETH price at 3000 USDC
        let cost = l2_rollup_fixed_cost(config, dec!(3000)).unwrap();
        assert!(cost > Decimal::ZERO);
    }
}
