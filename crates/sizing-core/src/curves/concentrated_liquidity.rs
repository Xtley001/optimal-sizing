//! Concentrated-liquidity (multi-tick and single-tick range) closed-form sizing.
//! Per `docs/CURVES_SPEC.md § 1`.
//!
//! # Mathematical Model
//! Within each contiguous tick interval `[sqrt_p_lower, sqrt_p_upper]` with active
//! liquidity `L`, the pool satisfies the virtual constant-product invariant:
//! `(x_virt) * (y_virt) = L^2` where `x_virt = L / sqrt_p` and `y_virt = L * sqrt_p`.
//!
//! Multi-tick quoting traverses tick boundaries piecewise: when a trade exhausts
//! the capacity of the current active tick, the residual input is executed against
//! the adjacent tick's active liquidity `L_next`.
//!
//! Because each tick's segment is strictly concave with strictly monotonic marginal price,
//! the global optimum is solved analytically in piecewise closed form (`GuaranteeTier::ProvenOptimal`).

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;

use crate::error::SizingError;
use crate::profit::NetProfit;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// A discrete tick range in a concentrated liquidity pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickRange {
    /// `sqrt(price)` at the lower tick bound.
    pub lower_sqrt_price: Decimal,
    /// `sqrt(price)` at the upper tick bound.
    pub upper_sqrt_price: Decimal,
    /// Gross active liquidity `L` in this tick range.
    pub liquidity_gross: Decimal,
    /// Net liquidity delta when crossing this tick boundary.
    pub liquidity_net: Decimal,
}

/// A concentrated liquidity pool supporting single-tick and multi-tick traversal.
#[derive(Debug, Clone)]
pub struct ConcentratedLiquidity {
    /// Active liquidity `L` at current price.
    pub liquidity: Decimal,
    /// `sqrt(price)` at the overall lower bound.
    pub sqrt_price_lower: Decimal,
    /// `sqrt(price)` at the overall upper bound.
    pub sqrt_price_upper: Decimal,
    /// `sqrt(price)` at the current pool state.
    pub sqrt_price_current: Decimal,
    /// Fee retention, e.g. `0.997` for a 30bps fee (`1 - fee`).
    pub fee_retention: Decimal,
    /// Initialized tick ranges comprising the pool, sorted in ascending order of price.
    pub tick_ranges: Vec<TickRange>,
}

impl ConcentratedLiquidity {
    /// Constructs a single-tick position (for backward compatibility).
    pub fn new(
        liquidity: Decimal,
        sqrt_price_lower: Decimal,
        sqrt_price_upper: Decimal,
        sqrt_price_current: Decimal,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError> {
        if liquidity <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("liquidity must be > 0, got {liquidity}"),
            });
        }
        if sqrt_price_lower <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("sqrt_price_lower must be > 0, got {sqrt_price_lower}"),
            });
        }
        if sqrt_price_upper <= sqrt_price_lower {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "sqrt_price_upper ({sqrt_price_upper}) must be > sqrt_price_lower ({sqrt_price_lower})"
                ),
            });
        }
        if sqrt_price_current < sqrt_price_lower || sqrt_price_current > sqrt_price_upper {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "sqrt_price_current ({sqrt_price_current}) is outside [{sqrt_price_lower}, {sqrt_price_upper}]"
                ),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }

        let tick_ranges = vec![TickRange {
            lower_sqrt_price: sqrt_price_lower,
            upper_sqrt_price: sqrt_price_upper,
            liquidity_gross: liquidity,
            liquidity_net: Decimal::ZERO,
        }];

        Ok(Self {
            liquidity,
            sqrt_price_lower,
            sqrt_price_upper,
            sqrt_price_current,
            fee_retention,
            tick_ranges,
        })
    }

    /// Constructs a multi-tick concentrated liquidity pool from discrete tick ranges.
    pub fn new_multi_tick(
        mut tick_ranges: Vec<TickRange>,
        sqrt_price_current: Decimal,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError> {
        if tick_ranges.is_empty() {
            return Err(SizingError::InvalidReserves {
                detail: "tick_ranges must not be empty".to_string(),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }

        tick_ranges.sort_by_key(|a| a.lower_sqrt_price);

        for (i, range) in tick_ranges.iter().enumerate() {
            if range.lower_sqrt_price <= Decimal::ZERO {
                return Err(SizingError::InvalidReserves {
                    detail: format!(
                        "tick range {i} lower_sqrt_price must be > 0, got {}",
                        range.lower_sqrt_price
                    ),
                });
            }
            if range.upper_sqrt_price <= range.lower_sqrt_price {
                return Err(SizingError::InvalidReserves {
                    detail: format!(
                        "tick range {i} upper_sqrt_price ({}) must be > lower_sqrt_price ({})",
                        range.upper_sqrt_price, range.lower_sqrt_price
                    ),
                });
            }
            if range.liquidity_gross <= Decimal::ZERO {
                return Err(SizingError::InvalidReserves {
                    detail: format!(
                        "tick range {i} liquidity_gross must be > 0, got {}",
                        range.liquidity_gross
                    ),
                });
            }
        }

        let overall_lower = tick_ranges.first().unwrap().lower_sqrt_price;
        let overall_upper = tick_ranges.last().unwrap().upper_sqrt_price;

        if sqrt_price_current < overall_lower || sqrt_price_current > overall_upper {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "sqrt_price_current ({sqrt_price_current}) is outside overall bounds [{overall_lower}, {overall_upper}]"
                ),
            });
        }

        let active_idx = tick_ranges
            .iter()
            .rposition(|r| {
                sqrt_price_current >= r.lower_sqrt_price && sqrt_price_current <= r.upper_sqrt_price
            })
            .ok_or_else(|| SizingError::InvalidReserves {
                detail: format!(
                    "current sqrt price {sqrt_price_current} does not fall within any tick range"
                ),
            })?;

        let current_liquidity = tick_ranges[active_idx].liquidity_gross;

        Ok(Self {
            liquidity: current_liquidity,
            sqrt_price_lower: overall_lower,
            sqrt_price_upper: overall_upper,
            sqrt_price_current,
            fee_retention,
            tick_ranges,
        })
    }

    /// `(virtual_x, virtual_y)` at current price in the active tick.
    pub fn virtual_reserves(&self) -> (Decimal, Decimal) {
        let virtual_x = self.liquidity / self.sqrt_price_current;
        let virtual_y = self.liquidity * self.sqrt_price_current;
        (virtual_x, virtual_y)
    }
}

impl PricingCurve for ConcentratedLiquidity {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        if delta_in == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        let mut remaining_in = delta_in;
        let mut current_p = self.sqrt_price_current;
        let mut total_out = Decimal::ZERO;

        let mut k = self
            .tick_ranges
            .iter()
            .rposition(|r| current_p >= r.lower_sqrt_price && current_p <= r.upper_sqrt_price)
            .ok_or_else(|| SizingError::InvalidReserves {
                detail: format!("price {current_p} not found in tick ranges"),
            })?;

        while remaining_in > Decimal::ZERO {
            let range = &self.tick_ranges[k];
            let l = range.liquidity_gross;
            let lower_p = range.lower_sqrt_price;

            let max_in_this_tick = (l / lower_p - l / current_p) / self.fee_retention;

            if remaining_in <= max_in_this_tick {
                let next_p = l / (l / current_p + self.fee_retention * remaining_in);
                let out_piece = l * (current_p - next_p);
                total_out += out_piece;
                remaining_in = Decimal::ZERO;
            } else {
                let out_piece = l * (current_p - lower_p);
                total_out += out_piece;
                remaining_in -= max_in_this_tick;
                current_p = lower_p;

                if k == 0 {
                    if remaining_in > Decimal::ZERO {
                        return Err(SizingError::InvalidReserves {
                            detail: format!(
                                "trade of size {delta_in} exceeds total liquidity capacity across all tick ranges"
                            ),
                        });
                    }
                } else {
                    k -= 1;
                }
            }
        }

        if total_out < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("computed negative output ({total_out}) for delta_in {delta_in}"),
            });
        }

        Ok(total_out)
    }
}

impl SizingAlgorithm for ConcentratedLiquidity {
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

        let initial_marginal_price =
            self.fee_retention * self.sqrt_price_current * self.sqrt_price_current;
        if initial_marginal_price <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let mut k = self
            .tick_ranges
            .iter()
            .rposition(|r| {
                self.sqrt_price_current >= r.lower_sqrt_price
                    && self.sqrt_price_current <= r.upper_sqrt_price
            })
            .ok_or_else(|| SizingError::InvalidReserves {
                detail: format!("price {} not found in tick ranges", self.sqrt_price_current),
            })?;

        let mut current_p = self.sqrt_price_current;
        let mut cumulative_delta_in = Decimal::ZERO;
        let chosen_delta = loop {
            let range = &self.tick_ranges[k];
            let l = range.liquidity_gross;
            let lower_p = range.lower_sqrt_price;

            let virtual_x = l / current_p;
            let virtual_y = l * current_p;

            let marginal_p = self.fee_retention * virtual_y / virtual_x;
            if marginal_p <= reference_price {
                break cumulative_delta_in;
            }

            let radicand = virtual_x * virtual_y * self.fee_retention / reference_price;
            let sqrt_radicand = match radicand.sqrt() {
                Some(s) => s,
                None => {
                    return Err(SizingError::InvalidReserves {
                        detail: format!("failed to compute sqrt of radicand {radicand}"),
                    })
                }
            };

            let unconstrained_delta_k = (sqrt_radicand - virtual_x) / self.fee_retention;
            let max_in_this_tick = (l / lower_p - virtual_x) / self.fee_retention;

            if unconstrained_delta_k <= max_in_this_tick {
                break cumulative_delta_in + unconstrained_delta_k.max(Decimal::ZERO);
            } else {
                cumulative_delta_in += max_in_this_tick;
                current_p = lower_p;
                if k == 0 {
                    break cumulative_delta_in;
                } else {
                    k -= 1;
                }
            }
        };

        let effective_constraints = constraints.clone();
        let (_, optimal_delta) =
            NetProfit::restrict_domain(&effective_constraints, (Decimal::ZERO, chosen_delta))?;

        if optimal_delta <= Decimal::ZERO {
            return Err(SizingError::NoProfitableSize);
        }

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
            iterations: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::Cpmm;
    use rust_decimal_macros::dec;

    #[test]
    fn wide_range_matches_equivalent_cpmm() {
        let liquidity = dec!(1_000_000);
        let sqrt_price_current = dec!(1.0);
        let sqrt_price_lower = dec!(0.5);
        let sqrt_price_upper = dec!(2.0);
        let fee_retention = dec!(0.997);

        let cl = ConcentratedLiquidity::new(
            liquidity,
            sqrt_price_lower,
            sqrt_price_upper,
            sqrt_price_current,
            fee_retention,
        )
        .unwrap();
        let (virtual_x, virtual_y) = cl.virtual_reserves();
        let equivalent_cpmm = Cpmm::new(virtual_x, virtual_y, fee_retention).unwrap();

        let reference_price = dec!(0.9);
        let cl_result = cl
            .optimal_size(
                reference_price,
                SizingConstraints {
                    fixed_cost: dec!(0),
                    max_size: None,
                },
            )
            .unwrap();
        let cpmm_result = equivalent_cpmm
            .optimal_size(
                reference_price,
                SizingConstraints {
                    fixed_cost: dec!(0),
                    max_size: None,
                },
            )
            .unwrap();

        let delta_diff = (cl_result.optimal_delta - cpmm_result.optimal_delta).abs();
        assert!(delta_diff <= cpmm_result.optimal_delta * dec!(0.0000000001));
        let profit_diff = (cl_result.expected_profit - cpmm_result.expected_profit).abs();
        assert!(profit_diff <= cpmm_result.expected_profit * dec!(0.0000000001));
        assert!(matches!(
            cl_result.guarantee_tier,
            GuaranteeTier::ProvenOptimal
        ));
    }

    #[test]
    fn multi_tick_crosses_boundary_successfully() {
        let tick1 = TickRange {
            lower_sqrt_price: dec!(0.90),
            upper_sqrt_price: dec!(1.00),
            liquidity_gross: dec!(1_000_000),
            liquidity_net: dec!(0),
        };
        let tick2 = TickRange {
            lower_sqrt_price: dec!(1.00),
            upper_sqrt_price: dec!(1.10),
            liquidity_gross: dec!(2_000_000),
            liquidity_net: dec!(0),
        };

        let pool =
            ConcentratedLiquidity::new_multi_tick(vec![tick1, tick2], dec!(1.05), dec!(0.997))
                .unwrap();

        // Size trade large enough to cross tick 2 into tick 1
        let quote_res = pool.quote(dec!(100_000));
        assert!(quote_res.is_ok());
        let output = quote_res.unwrap();
        assert!(output > dec!(0));
    }

    #[test]
    fn multi_tick_optimal_sizing_finds_cross_tick_optimum() {
        let tick1 = TickRange {
            lower_sqrt_price: dec!(0.80),
            upper_sqrt_price: dec!(1.00),
            liquidity_gross: dec!(1_000_000),
            liquidity_net: dec!(0),
        };
        let tick2 = TickRange {
            lower_sqrt_price: dec!(1.00),
            upper_sqrt_price: dec!(1.20),
            liquidity_gross: dec!(500_000),
            liquidity_net: dec!(0),
        };

        let pool =
            ConcentratedLiquidity::new_multi_tick(vec![tick1, tick2], dec!(1.10), dec!(0.997))
                .unwrap();

        let reference_price = dec!(0.85);
        let res = pool
            .optimal_size(
                reference_price,
                SizingConstraints {
                    fixed_cost: dec!(0),
                    max_size: None,
                },
            )
            .unwrap();

        assert!(res.optimal_delta > dec!(0));
        assert!(res.expected_profit > dec!(0));
        assert!(matches!(res.guarantee_tier, GuaranteeTier::ProvenOptimal));
    }
}
