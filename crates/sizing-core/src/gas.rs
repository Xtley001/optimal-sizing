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
}
