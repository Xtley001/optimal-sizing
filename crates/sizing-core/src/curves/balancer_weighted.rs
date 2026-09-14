//! Balancer Weighted Pools ($V = \prod_{i=1}^n B_i^{w_i}$) closed-form sizing.
//! Per `docs/CURVES_SPEC.md § 2`.
//!
//! # Mathematical Model
//! For an $n$-asset pool with weights $\sum w_i = 1$, swapping token $i$ (reserve $B_i$, weight $w_i$)
//! for token $o$ (reserve $B_o$, weight $w_o$) with fee retention $f \in (0, 1]$ satisfies:
//!
//! $$\text{quote}(\Delta x) = B_o \cdot \left[1 - \left(\frac{B_i}{B_i + f \cdot \Delta x}\right)^{\frac{w_i}{w_o}}\right]$$
//!
//! # Strict Concavity & Exact Closed-Form Solution
//! As derived in `docs/CURVES_SPEC.md § 2.2`, $\text{Profit}''(\Delta x) < 0$ strictly for all $\Delta x \ge 0$.
//! Setting $\text{Profit}'(\Delta x) = 0$ yields the unique global analytical optimum:
//!
//! $$\Delta x^* = \frac{B_i \cdot \left[\left(\frac{w_i \cdot B_o \cdot f}{w_o \cdot B_i \cdot P}\right)^{\frac{w_o}{w_i + w_o}} - 1\right]}{f}$$
//!
//! Guarantee Tier: `GuaranteeTier::ProvenOptimal`.

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;

use crate::error::SizingError;
use crate::profit::NetProfit;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// A Balancer weighted constant-value pool ($V = \prod B_i^{w_i}$).
#[derive(Debug, Clone)]
pub struct BalancerWeightedPool {
    /// Token balance reserves $B_i$.
    pub reserves: Vec<Decimal>,
    /// Normalized token weights $w_i > 0, \sum w_i = 1$.
    pub weights: Vec<Decimal>,
    /// Fee retention $f \in (0, 1]$ (e.g. `0.997` for 30bps fee).
    pub fee_retention: Decimal,
    /// Index of input token $i$ being sold to the pool.
    pub input_index: usize,
    /// Index of output token $o$ being bought from the pool.
    pub output_index: usize,
}

impl BalancerWeightedPool {
    /// Constructs a new `BalancerWeightedPool`.
    pub fn new(
        reserves: Vec<Decimal>,
        weights: Vec<Decimal>,
        fee_retention: Decimal,
        input_index: usize,
        output_index: usize,
    ) -> Result<Self, SizingError> {
        if reserves.len() < 2 {
            return Err(SizingError::InvalidReserves {
                detail: format!("pool must have at least 2 assets, got {}", reserves.len()),
            });
        }
        if reserves.len() != weights.len() {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "reserves count ({}) must match weights count ({})",
                    reserves.len(),
                    weights.len()
                ),
            });
        }
        if input_index >= reserves.len() || output_index >= reserves.len() {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "indices out of bounds: input_index={input_index}, output_index={output_index}, total={}",
                    reserves.len()
                ),
            });
        }
        if input_index == output_index {
            return Err(SizingError::InvalidReserves {
                detail: "input_index and output_index must be distinct".to_string(),
            });
        }

        for (i, &r) in reserves.iter().enumerate() {
            if r <= Decimal::ZERO {
                return Err(SizingError::InvalidReserves {
                    detail: format!("reserve {i} must be > 0, got {r}"),
                });
            }
        }

        let mut weight_sum = Decimal::ZERO;
        for (i, &w) in weights.iter().enumerate() {
            if w <= Decimal::ZERO {
                return Err(SizingError::InvalidReserves {
                    detail: format!("weight {i} must be > 0, got {w}"),
                });
            }
            weight_sum += w;
        }

        if (weight_sum - Decimal::ONE).abs() > Decimal::from_str_exact("0.01").unwrap() {
            return Err(SizingError::InvalidReserves {
                detail: format!("weights must sum to approximately 1.0, got sum {weight_sum}"),
            });
        }

        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }

        Ok(Self {
            reserves,
            weights,
            fee_retention,
            input_index,
            output_index,
        })
    }

    /// Input token reserve $B_i$.
    #[inline]
    pub fn input_reserve(&self) -> Decimal {
        self.reserves[self.input_index]
    }

    /// Output token reserve $B_o$.
    #[inline]
    pub fn output_reserve(&self) -> Decimal {
        self.reserves[self.output_index]
    }

    /// Input token weight $w_i$.
    #[inline]
    pub fn input_weight(&self) -> Decimal {
        self.weights[self.input_index]
    }

    /// Output token weight $w_o$.
    #[inline]
    pub fn output_weight(&self) -> Decimal {
        self.weights[self.output_index]
    }
}

impl PricingCurve for BalancerWeightedPool {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        if delta_in == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        let bi = self.input_reserve();
        let bo = self.output_reserve();
        let wi = self.input_weight();
        let wo = self.output_weight();

        let denominator = bi + self.fee_retention * delta_in;
        let base = bi / denominator;
        let exponent = wi / wo;

        let factor = match base.checked_powd(exponent) {
            Some(f) => f,
            None => {
                return Err(SizingError::InvalidReserves {
                    detail: format!("failed to compute powd({base}, {exponent}) in Balancer quote"),
                })
            }
        };

        let output = bo * (Decimal::ONE - factor);
        if output < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("computed negative output ({output}) for delta_in {delta_in}"),
            });
        }

        Ok(output)
    }
}

impl SizingAlgorithm for BalancerWeightedPool {
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

        let bi = self.input_reserve();
        let bo = self.output_reserve();
        let wi = self.input_weight();
        let wo = self.output_weight();
        let f = self.fee_retention;

        // Marginal price at delta_in = 0: (wi / wo) * (bo * f / bi)
        let marginal_price = (wi / wo) * (bo * f / bi);
        if marginal_price <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        // Exact closed form:
        // Delta x* = Bi * [ ((wi * bo * f) / (wo * bi * P))^(wo / (wi + wo)) - 1 ] / f
        let base = (wi * bo * f) / (wo * bi * reference_price);
        let exponent = wo / (wi + wo);

        let term = match base.checked_powd(exponent) {
            Some(t) => t,
            None => {
                return Err(SizingError::InvalidReserves {
                    detail: format!(
                        "failed to compute powd({base}, {exponent}) in Balancer sizing"
                    ),
                })
            }
        };

        let unconstrained_delta = (bi * (term - Decimal::ONE)) / f;
        if unconstrained_delta <= Decimal::ZERO {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let (_, optimal_delta) =
            NetProfit::restrict_domain(&constraints, (Decimal::ZERO, unconstrained_delta))?;

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
    fn fifty_fifty_pool_matches_cpmm_exactly() {
        let bi = dec!(1_000_000);
        let bo = dec!(2_000_000);
        let f = dec!(0.997);

        let balancer =
            BalancerWeightedPool::new(vec![bi, bo], vec![dec!(0.5), dec!(0.5)], f, 0, 1).unwrap();

        let cpmm = Cpmm::new(bi, bo, f).unwrap();

        let delta = dec!(10_000);
        let b_out = balancer.quote(delta).unwrap();
        let c_out = cpmm.quote(delta).unwrap();

        let diff = (b_out - c_out).abs();
        assert!(diff <= c_out * dec!(0.000000001));

        let ref_price = dec!(1.5);
        let b_opt = balancer
            .optimal_size(
                ref_price,
                SizingConstraints {
                    fixed_cost: dec!(0),
                    max_size: None,
                },
            )
            .unwrap();
        let c_opt = cpmm
            .optimal_size(
                ref_price,
                SizingConstraints {
                    fixed_cost: dec!(0),
                    max_size: None,
                },
            )
            .unwrap();

        let opt_diff = (b_opt.optimal_delta - c_opt.optimal_delta).abs();
        assert!(opt_diff <= c_opt.optimal_delta * dec!(0.000000001));
        assert!(matches!(b_opt.guarantee_tier, GuaranteeTier::ProvenOptimal));
    }

    #[test]
    fn asymmetric_80_20_pool_sizing() {
        let balancer = BalancerWeightedPool::new(
            vec![dec!(800_000), dec!(200_000)],
            vec![dec!(0.8), dec!(0.2)],
            dec!(0.997),
            0,
            1,
        )
        .unwrap();

        let ref_price = dec!(0.5);
        let res = balancer
            .optimal_size(
                ref_price,
                SizingConstraints {
                    fixed_cost: dec!(0),
                    max_size: None,
                },
            )
            .unwrap();

        assert!(res.optimal_delta > dec!(0));
        assert!(res.expected_profit > dec!(0));
    }
}
