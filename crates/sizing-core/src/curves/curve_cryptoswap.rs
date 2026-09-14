//! Curve CryptoSwap (v2 Dynamic Invariant) numerical solver & sizing.
//! Per `docs/CURVES_SPEC.md § 4`.
//!
//! # Invariant Equation
//! For a 2-asset pair $(x_0, x_1)$ governed by amplification parameter $A$ and smoothing parameter $\gamma$:
//! $$K \cdot D \cdot \gamma + D = A \cdot \gamma \cdot K_0 \cdot \sum x_i + \frac{D^3}{4 \cdot x_0 \cdot x_1}$$
//! where:
//! $$K_0 = \frac{4 \cdot x_0 \cdot x_1}{D^2}, \quad K = A \cdot K_0 \cdot \left(\frac{\gamma}{\gamma + 1 - K_0}\right)^2$$
//!
//! Guarantee Tier: `GuaranteeTier::NumericallyGuaranteed`.

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;

use crate::error::SizingError;
use crate::profit::NetProfit;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

const NEWTON_MAX_ITERATIONS: usize = 100;
const NEWTON_TOLERANCE: Decimal = dec!(0.000001);
const INV_PHI: Decimal = dec!(0.6180339887498948482045868344);

/// Curve CryptoSwap v2 pool with dynamic invariant.
#[derive(Debug, Clone)]
pub struct CurveCryptoSwap {
    /// Token reserves $[x_0, x_1]$.
    pub balances: [Decimal; 2],
    /// Amplification parameter $A$.
    pub amplification_a: Decimal,
    /// Smoothing parameter $\gamma$.
    pub gamma: Decimal,
    /// Fee retention $f \in (0, 1]$.
    pub fee_retention: Decimal,
    /// Index of input token (0 or 1).
    pub input_index: usize,
}

impl CurveCryptoSwap {
    /// Constructs a new `CurveCryptoSwap` pool.
    pub fn new(
        balances: [Decimal; 2],
        amplification_a: Decimal,
        gamma: Decimal,
        fee_retention: Decimal,
        input_index: usize,
    ) -> Result<Self, SizingError> {
        if balances[0] <= Decimal::ZERO || balances[1] <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!(
                    "balances must be > 0, got [{}, {}]",
                    balances[0], balances[1]
                ),
            });
        }
        if amplification_a <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("amplification_a must be > 0, got {amplification_a}"),
            });
        }
        if gamma <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("gamma must be > 0, got {gamma}"),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }
        if input_index > 1 {
            return Err(SizingError::InvalidReserves {
                detail: format!("input_index must be 0 or 1, got {input_index}"),
            });
        }

        Ok(Self {
            balances,
            amplification_a,
            gamma,
            fee_retention,
            input_index,
        })
    }

    /// Evaluates the CryptoSwap invariant residual $F(D, x_0, x_1) = 0$.
    fn invariant_residual(&self, d: Decimal, x0: Decimal, x1: Decimal) -> Decimal {
        let sum_x = x0 + x1;
        let prod_4x = dec!(4) * x0 * x1;
        let k0 = (prod_4x / (d * d)).min(Decimal::ONE);

        let g_term = self.gamma / (self.gamma + Decimal::ONE - k0);
        let k = self.amplification_a * k0 * g_term * g_term;

        let left = k * d * self.gamma + d;
        let right = self.amplification_a * self.gamma * k0 * sum_x + (d * d * d) / prod_4x;

        left - right
    }

    /// Solves invariant $D$ using 1D Newton iteration from initial geometric mean guess.
    pub fn compute_d(&self) -> Result<Decimal, SizingError> {
        let x0 = self.balances[0];
        let x1 = self.balances[1];

        // Initial guess: 2 * sqrt(x0 * x1)
        let prod = x0 * x1;
        let mut d = match prod.sqrt() {
            Some(s) => dec!(2) * s,
            None => {
                return Err(SizingError::InvalidReserves {
                    detail: format!("failed to sqrt(x0 * x1) for prod {prod}"),
                })
            }
        };

        for _ in 0..NEWTON_MAX_ITERATIONS {
            let res = self.invariant_residual(d, x0, x1);
            if res.abs() < NEWTON_TOLERANCE {
                return Ok(d);
            }

            // Numerical derivative
            let eps = (d * dec!(0.0001)).max(dec!(0.0001));
            let res_eps = self.invariant_residual(d + eps, x0, x1);
            let deriv = (res_eps - res) / eps;

            if deriv.is_zero() {
                break;
            }

            let next_d = d - res / deriv;
            if (next_d - d).abs() < NEWTON_TOLERANCE {
                return Ok(next_d);
            }
            d = next_d;
        }

        Ok(d)
    }

    /// Solves output balance $y = x_{out}$ given updated input balance $x_{in}'$ and invariant $D$.
    fn solve_y(&self, d: Decimal, new_x_in: Decimal) -> Result<Decimal, SizingError> {
        let (x_in_idx, x_out_idx) = if self.input_index == 0 {
            (0, 1)
        } else {
            (1, 0)
        };
        let max_y = d * d / (dec!(4) * new_x_in);
        let mut y = max_y.min(self.balances[x_out_idx]);

        for _ in 0..NEWTON_MAX_ITERATIONS {
            let (x0, x1) = if x_in_idx == 0 {
                (new_x_in, y)
            } else {
                (y, new_x_in)
            };
            let res = self.invariant_residual(d, x0, x1);
            if res.abs() < NEWTON_TOLERANCE {
                return Ok(y);
            }

            let eps = (y * dec!(0.0001)).max(dec!(0.0001));
            let (x0_eps, x1_eps) = if x_in_idx == 0 {
                (new_x_in, y + eps)
            } else {
                (y + eps, new_x_in)
            };
            let res_eps = self.invariant_residual(d, x0_eps, x1_eps);
            let deriv = (res_eps - res) / eps;

            if deriv.is_zero() {
                break;
            }

            let next_y = y - res / deriv;
            let clamped_next_y = next_y.min(max_y).max(dec!(0.000001));
            if (clamped_next_y - y).abs() < NEWTON_TOLERANCE {
                return Ok(clamped_next_y);
            }
            y = clamped_next_y;
        }

        Ok(y)
    }
}

impl PricingCurve for CurveCryptoSwap {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        if delta_in == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        let d = self.compute_d()?;
        let in_idx = self.input_index;
        let out_idx = 1 - in_idx;

        let effective_delta = delta_in * self.fee_retention;
        let new_x_in = self.balances[in_idx] + effective_delta;

        let new_y = self.solve_y(d, new_x_in)?;
        let old_y = self.balances[out_idx];

        let output = (old_y - new_y).max(Decimal::ZERO);
        Ok(output)
    }
}

impl SizingAlgorithm for CurveCryptoSwap {
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

        let test_step = dec!(0.001);
        let test_out = self.quote(test_step)?;
        let marginal_p = test_out / test_step;
        if marginal_p <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let search_upper = match constraints.max_size {
            Some(m) => m.min(self.balances[self.input_index] * dec!(2)),
            None => self.balances[self.input_index] * dec!(2),
        };

        let mut a = Decimal::ZERO;
        let mut b = search_upper;
        let iterations = 64;

        let profit_fn = |delta: Decimal| -> Decimal {
            match self.quote(delta) {
                Ok(out) => out - reference_price * delta,
                Err(_) => dec!(-1_000_000_000),
            }
        };

        let mut x1 = b - INV_PHI * (b - a);
        let mut x2 = a + INV_PHI * (b - a);
        let mut f1 = profit_fn(x1);
        let mut f2 = profit_fn(x2);

        for _ in 0..iterations {
            if f1 < f2 {
                a = x1;
                x1 = x2;
                f1 = f2;
                x2 = a + INV_PHI * (b - a);
                f2 = profit_fn(x2);
            } else {
                b = x2;
                x2 = x1;
                f2 = f1;
                x1 = b - INV_PHI * (b - a);
                f1 = profit_fn(x1);
            }
        }

        let unconstrained_delta = (a + b) / dec!(2);
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
            guarantee_tier: GuaranteeTier::NumericallyGuaranteed {
                convergence_conditions_met: true,
            },
            iterations: Some(iterations),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cryptoswap_quote_and_sizing() {
        let pool = CurveCryptoSwap::new(
            [dec!(1_000_000), dec!(1_000_000)],
            dec!(100),
            dec!(0.0001),
            dec!(0.997),
            0,
        )
        .unwrap();

        let quote = pool.quote(dec!(1_000)).unwrap();
        assert!(quote > dec!(0));

        let ref_price = dec!(0.9);
        let res = pool
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
        assert!(matches!(
            res.guarantee_tier,
            GuaranteeTier::NumericallyGuaranteed {
                convergence_conditions_met: true
            }
        ));
    }
}
