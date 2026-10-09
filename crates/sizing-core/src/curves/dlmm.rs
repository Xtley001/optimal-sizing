//! Trader Joe / Meteora DLMM (Discretized Liquidity Book) AMM curve.
//!
//! # Mathematical Model
//! Liquidity is partitioned into discrete bins. Within each bin `i`, tokens swap along
//! a constant-sum invariant with zero price impact:
//! `delta_y = P_i * f * delta_x`
//! where `P_i` is the bin price and `f` is the fee retention factor.
//!
//! When a trade exhausts a bin's available reserve `Y_i`, residual input traverses to the next
//! lower-priced bin.
//!
//! Because marginal return is a monotonically decreasing step function, the profit function
//! is piecewise concave. The optimal size is solved analytically by aggregating capacity across
//! all bins clearing the reference price (`GuaranteeTier::ProvenOptimal`).

use rust_decimal::Decimal;

use crate::error::SizingError;
use crate::profit::NetProfit;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// An individual discrete liquidity bin in a DLMM pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlmmBin {
    /// Unique bin identifier.
    pub bin_id: i32,
    /// Price of the bin (output units Y per input unit X). Must be strictly positive.
    pub price: Decimal,
    /// Reserve of input token X available in this bin.
    pub reserve_x: Decimal,
    /// Reserve of output token Y available in this bin.
    pub reserve_y: Decimal,
}

/// Discretized Liquidity Book (DLMM) curve.
#[derive(Debug, Clone)]
pub struct LiquidityBook {
    /// Active bin index in `bins` where trading begins.
    pub active_bin_index: usize,
    /// Initialized bins, sorted in descending order of price for selling X for Y.
    pub bins: Vec<DlmmBin>,
    /// Fee retention factor f (e.g. `0.998` for 20bps fee). In (0, 1].
    pub fee_retention: Decimal,
}

impl LiquidityBook {
    /// Constructs a new `LiquidityBook` curve.
    ///
    /// Returns `SizingError::InvalidReserves` if `bins` is empty, `active_bin_index` is out of bounds,
    /// any bin has non-positive price or negative reserves, or `fee_retention` is not in `(0, 1]`.
    pub fn new(
        bins: Vec<DlmmBin>,
        active_bin_index: usize,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError> {
        if bins.is_empty() {
            return Err(SizingError::InvalidReserves {
                detail: "bins must not be empty".to_string(),
            });
        }
        if active_bin_index >= bins.len() {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "active_bin_index {} out of bounds for bins len {}",
                    active_bin_index,
                    bins.len()
                ),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }

        for (idx, bin) in bins.iter().enumerate() {
            if bin.price <= Decimal::ZERO {
                return Err(SizingError::InvalidReserves {
                    detail: format!("bin {} has price <= 0: {}", idx, bin.price),
                });
            }
            if bin.reserve_x < Decimal::ZERO || bin.reserve_y < Decimal::ZERO {
                return Err(SizingError::InvalidReserves {
                    detail: format!("bin {} has negative reserves", idx),
                });
            }
        }

        Ok(Self {
            active_bin_index,
            bins,
            fee_retention,
        })
    }

    /// Total output reserve Y available across all active and subsequent bins.
    pub fn total_reserve_y(&self) -> Decimal {
        self.bins[self.active_bin_index..]
            .iter()
            .map(|b| b.reserve_y)
            .sum()
    }
}

impl PricingCurve for LiquidityBook {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        if delta_in == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        let mut rem_input = delta_in;
        let mut total_output = Decimal::ZERO;

        for bin in &self.bins[self.active_bin_index..] {
            if rem_input <= Decimal::ZERO {
                break;
            }
            if bin.reserve_y <= Decimal::ZERO {
                continue;
            }

            // Input needed to exhaust this bin: capacity = bin.reserve_y / (bin.price * fee)
            let effective_price = bin.price * self.fee_retention;
            let bin_input_cap = bin.reserve_y / effective_price;

            if rem_input <= bin_input_cap {
                total_output += rem_input * effective_price;
                rem_input = Decimal::ZERO;
            } else {
                total_output += bin.reserve_y;
                rem_input -= bin_input_cap;
            }
        }

        Ok(total_output)
    }
}

impl SizingAlgorithm for LiquidityBook {
    fn optimal_size(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError> {
        if reference_price <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("reference_price must be > 0, got {reference_price}"),
            });
        }

        // Active bin check: does the first bin clear reference_price?
        let active_bin = &self.bins[self.active_bin_index];
        if active_bin.price * self.fee_retention <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        // Aggregate capacity of all consecutive bins clearing reference_price:
        let mut optimal_delta = Decimal::ZERO;
        let mut raw_profit = Decimal::ZERO;

        for bin in &self.bins[self.active_bin_index..] {
            let effective_price = bin.price * self.fee_retention;
            if effective_price <= reference_price {
                break; // Stop at boundary where marginal profit ceases to be positive
            }
            if bin.reserve_y <= Decimal::ZERO {
                continue;
            }

            let bin_cap = bin.reserve_y / effective_price;
            optimal_delta += bin_cap;
            raw_profit += bin.reserve_y - reference_price * bin_cap;
        }

        if optimal_delta <= Decimal::ZERO {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let (_, optimal_delta) =
            NetProfit::restrict_domain(&constraints, (Decimal::ZERO, optimal_delta))?;

        let output = self.quote(optimal_delta)?;
        let gross_profit = output - reference_price * optimal_delta;
        let expected_profit = gross_profit - constraints.fixed_cost;
        if expected_profit <= Decimal::ZERO {
            return Err(SizingError::NoProfitableSize);
        }

        Ok(SizingResult {
            optimal_delta,
            expected_profit,
            guarantee_tier: GuaranteeTier::ProvenOptimal,
            iterations: None, // Exact closed form!
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn dlmm_quote_and_sizing_across_bins() {
        let bins = vec![
            DlmmBin {
                bin_id: 100,
                price: dec!(1.05),
                reserve_x: dec!(0),
                reserve_y: dec!(1050), // absorbs 1000 input at 1.05
            },
            DlmmBin {
                bin_id: 99,
                price: dec!(1.00),
                reserve_x: dec!(0),
                reserve_y: dec!(2000), // absorbs 2000 input at 1.00
            },
        ];

        let book = LiquidityBook::new(bins, 0, dec!(1.0)).unwrap();

        // Quote 500 input: entirely within bin 100 at 1.05
        assert_eq!(book.quote(dec!(500)).unwrap(), dec!(525));

        // Quote 1500 input: exhausts bin 100 (1050 output) + 500 in bin 99 (500 output) = 1550 output
        assert_eq!(book.quote(dec!(1500)).unwrap(), dec!(1550));

        // Sizing against reference price 1.02:
        // Bin 100 price (1.05) > 1.02 -> take all 1000 input.
        // Bin 99 price (1.00) < 1.02 -> do not enter bin 99.
        let constraints = SizingConstraints {
            fixed_cost: dec!(5),
            max_size: None,
        };
        let res = book.optimal_size(dec!(1.02), constraints).unwrap();

        assert_eq!(res.optimal_delta, dec!(1000));
        assert_eq!(res.guarantee_tier, GuaranteeTier::ProvenOptimal);
        assert_eq!(res.iterations, None);
    }
}
