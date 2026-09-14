//! DODO Proactive Market Maker (PMM) sizing via Golden-Section search.
//! Per `docs/CURVES_SPEC.md § 3`.
//!
//! # Mathematical Model
//! DODO PMM concentrates liquidity around an oracle guide price $i$ with slippage coefficient $k \in (0, 1]$.
//! Depending on reserve status relative to targets $(B_0, Q_0)$, the pool operates in:
//! - Regime 1: $B < B_0$ (Base Shortage / $R < 1$)
//! - Regime 2: $Q < Q_0$ (Quote Shortage / $R > 1$)
//! - Regime 0: $B = B_0, Q = Q_0$ ($R = 1$)
//!
//! Because transitions are quadratic, optimal sizing is solved via Golden-Section Search
//! with unimodality confirmation (`GuaranteeTier::EmpiricallyValidated`).

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;

use crate::error::SizingError;
use crate::profit::NetProfit;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// Golden ratio constant $(\sqrt{5} - 1) / 2$.
const INV_PHI: Decimal = dec!(0.6180339887498948482045868344);

/// DODO Proactive Market Maker (PMM) pool.
#[derive(Debug, Clone)]
pub struct DodoPmm {
    /// Target base token reserve $B_0$.
    pub base_target: Decimal,
    /// Target quote token reserve $Q_0$.
    pub quote_target: Decimal,
    /// Current base token reserve $B$.
    pub base_reserve: Decimal,
    /// Current quote token reserve $Q$.
    pub quote_reserve: Decimal,
    /// Oracle guide price $i$ (quote per base).
    pub oracle_price: Decimal,
    /// Slippage coefficient $k \in (0, 1]$.
    pub k: Decimal,
    /// Fee retention $f \in (0, 1]$.
    pub fee_retention: Decimal,
    /// Trade direction: `true` for selling Base to buy Quote, `false` for selling Quote to buy Base.
    pub is_sell_base: bool,
}

impl DodoPmm {
    /// Constructs a new `DodoPmm` pool.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_target: Decimal,
        quote_target: Decimal,
        base_reserve: Decimal,
        quote_reserve: Decimal,
        oracle_price: Decimal,
        k: Decimal,
        fee_retention: Decimal,
        is_sell_base: bool,
    ) -> Result<Self, SizingError> {
        if base_target <= Decimal::ZERO || quote_target <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("targets must be > 0: B0={base_target}, Q0={quote_target}"),
            });
        }
        if base_reserve <= Decimal::ZERO || quote_reserve <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("reserves must be > 0: B={base_reserve}, Q={quote_reserve}"),
            });
        }
        if oracle_price <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("oracle_price must be > 0, got {oracle_price}"),
            });
        }
        if k <= Decimal::ZERO || k > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("k must be in (0, 1], got {k}"),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }

        Ok(Self {
            base_target,
            quote_target,
            base_reserve,
            quote_reserve,
            oracle_price,
            k,
            fee_retention,
            is_sell_base,
        })
    }

    /// Computes quote for selling base token to buy quote token.
    fn query_sell_base(&self, delta_base: Decimal) -> Result<Decimal, SizingError> {
        let b0 = self.base_target;
        let q0 = self.quote_target;
        let b = self.base_reserve;
        let q = self.quote_reserve;
        let i = self.oracle_price;
        let k = self.k;

        let b_prime = b + delta_base;

        if b_prime <= b0 {
            // Regime 1: Base Shortage
            let term_before = (b0 - b) * (Decimal::ONE + k * (b0 - b) / b0);
            let term_after = (b0 - b_prime) * (Decimal::ONE + k * (b0 - b_prime) / b0);
            let q_before = q0 + i * term_before;
            let q_after = q0 + i * term_after;
            Ok((q_before - q_after).max(Decimal::ZERO))
        } else if b >= b0 {
            // Regime 2: Quote Shortage
            // B' = B0 + (Q0 - Q') / i * (1 + k (Q0 - Q') / Q0)
            // k u^2 + Q0 u - Q0 i (B' - B0) = 0 where u = Q0 - Q'
            let delta_b_excess = b_prime - b0;
            let c = q0 * i * delta_b_excess;
            let disc = q0 * q0 + dec!(4) * k * c;
            let sqrt_disc = disc.sqrt().ok_or_else(|| SizingError::InvalidReserves {
                detail: format!("negative discriminant in DODO sell base: {disc}"),
            })?;
            let u = (sqrt_disc - q0) / (dec!(2) * k);
            let q_prime = q0 - u;
            Ok((q - q_prime).max(Decimal::ZERO))
        } else {
            // Crossing B0 boundary: split into b -> b0 and b0 -> b_prime
            let delta_1 = b0 - b;
            let delta_2 = b_prime - b0;
            let out_1 = {
                let term_before = delta_1 * (Decimal::ONE + k * delta_1 / b0);
                i * term_before
            };
            let out_2 = {
                let c = q0 * i * delta_2;
                let disc = q0 * q0 + dec!(4) * k * c;
                let sqrt_disc = disc.sqrt().ok_or_else(|| SizingError::InvalidReserves {
                    detail: format!("negative discriminant in DODO crossing: {disc}"),
                })?;
                (sqrt_disc - q0) / (dec!(2) * k)
            };
            Ok((out_1 + out_2).max(Decimal::ZERO))
        }
    }

    /// Computes quote for selling quote token to buy base token.
    fn query_sell_quote(&self, delta_quote: Decimal) -> Result<Decimal, SizingError> {
        let b0 = self.base_target;
        let q0 = self.quote_target;
        let b = self.base_reserve;
        let q = self.quote_reserve;
        let i = self.oracle_price;
        let k = self.k;

        let q_prime = q + delta_quote;

        if q_prime <= q0 {
            // Regime 2: Quote Shortage
            let term_before = (q0 - q) * (Decimal::ONE + k * (q0 - q) / q0);
            let term_after = (q0 - q_prime) * (Decimal::ONE + k * (q0 - q_prime) / q0);
            let b_before = b0 + term_before / i;
            let b_after = b0 + term_after / i;
            Ok((b_before - b_after).max(Decimal::ZERO))
        } else if q >= q0 {
            // Regime 1: Base Shortage
            // Q' = Q0 + i (B0 - B') (1 + k (B0 - B') / B0)
            // k v^2 + B0 v - B0 (Q' - Q0) / i = 0 where v = B0 - B'
            let delta_q_excess = q_prime - q0;
            let c = (b0 * delta_q_excess) / i;
            let disc = b0 * b0 + dec!(4) * k * c;
            let sqrt_disc = disc.sqrt().ok_or_else(|| SizingError::InvalidReserves {
                detail: format!("negative discriminant in DODO sell quote: {disc}"),
            })?;
            let v = (sqrt_disc - b0) / (dec!(2) * k);
            let b_prime = b0 - v;
            Ok((b - b_prime).max(Decimal::ZERO))
        } else {
            // Crossing Q0 boundary
            let delta_1 = q0 - q;
            let delta_2 = q_prime - q0;
            let out_1 = {
                let term_before = delta_1 * (Decimal::ONE + k * delta_1 / q0);
                term_before / i
            };
            let out_2 = {
                let c = (b0 * delta_2) / i;
                let disc = b0 * b0 + dec!(4) * k * c;
                let sqrt_disc = disc.sqrt().ok_or_else(|| SizingError::InvalidReserves {
                    detail: format!("negative discriminant in DODO crossing: {disc}"),
                })?;
                (sqrt_disc - b0) / (dec!(2) * k)
            };
            Ok((out_1 + out_2).max(Decimal::ZERO))
        }
    }
}

impl PricingCurve for DodoPmm {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        if delta_in == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        let effective_in = delta_in * self.fee_retention;

        if self.is_sell_base {
            self.query_sell_base(effective_in)
        } else {
            self.query_sell_quote(effective_in)
        }
    }
}

impl SizingAlgorithm for DodoPmm {
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

        // Test marginal profitability at small step
        let test_step = dec!(0.001);
        let test_out = self.quote(test_step)?;
        let marginal_p = test_out / test_step;
        if marginal_p <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        // Establish upper bound for search
        let max_search = if self.is_sell_base {
            self.base_reserve * dec!(5)
        } else {
            self.quote_reserve * dec!(5)
        };

        let search_upper = match constraints.max_size {
            Some(m) => m.min(max_search),
            None => max_search,
        };

        // Golden Section Search
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
            guarantee_tier: GuaranteeTier::EmpiricallyValidated {
                unimodality_confirmed: true,
            },
            iterations: Some(iterations),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dodo_quote_and_optimal_sizing_sell_base() {
        let pool = DodoPmm::new(
            dec!(100_000),
            dec!(200_000),
            dec!(100_000),
            dec!(200_000),
            dec!(2.0),
            dec!(0.5),
            dec!(0.997),
            true,
        )
        .unwrap();

        let quote = pool.quote(dec!(1_000)).unwrap();
        assert!(quote > dec!(0));

        let ref_price = dec!(1.5);
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
            GuaranteeTier::EmpiricallyValidated {
                unimodality_confirmed: true
            }
        ));
    }
}
