//! `sizing-portfolio` — Constrained multi-pool portfolio capital allocation across heterogeneous AMM curves.
//!
//! # Mathematical Model
//! Given `M` concurrent independent arbitrage opportunities, solve:
//! `max sum_i Profit_i(delta_i)`
//! subject to:
//! `sum_i delta_i <= TotalCapitalBudget`, with `delta_i >= 0`.
//!
//! Solved via dual water-filling: by KKT conditions, at the optimal allocation there exists
//! a shadow price `lambda >= 0` such that for every active pool:
//! `quote_i'(delta_i) = ReferencePrice_i + lambda`.
//!
//! This is equivalent to sizing each pool independently against the elevated reference price
//! `P_i + lambda`. Bisection over `lambda` converges in logarithmic time.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use sizing_core::error::SizingError;
use sizing_core::traits::SizingAlgorithm;
use sizing_core::types::{GuaranteeTier, SizingConstraints};

/// An individual arbitrage opportunity candidate in a multi-pool portfolio.
pub struct PortfolioOpportunity {
    /// Identifier for this opportunity (e.g. "uniswap_v2_usdc_weth").
    pub id: String,
    /// Pool implementing `SizingAlgorithm`.
    pub pool: Box<dyn SizingAlgorithm + Send + Sync>,
    /// Base external reference price for this pool.
    pub reference_price: Decimal,
    /// Execution constraints (fixed gas cost, max size limit).
    pub constraints: SizingConstraints,
}

/// Allocated capital and expected profit for an opportunity in the solved portfolio.
#[derive(Debug, Clone, PartialEq)]
pub struct AllocatedPosition {
    /// Identifier of the opportunity.
    pub id: String,
    /// Allocated optimal capital size Δx_i*.
    pub allocated_size: Decimal,
    /// Expected net profit from this allocation.
    pub expected_profit: Decimal,
    /// Individual guarantee tier from the underlying pool.
    pub guarantee_tier: GuaranteeTier,
}

/// The global result of multi-pool portfolio optimization.
#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioResult {
    /// Total capital deployed across all opportunities: sum(Δx_i*).
    pub total_capital_used: Decimal,
    /// Total expected profit net of all execution costs: sum(expected_profit_i).
    pub total_expected_profit: Decimal,
    /// Equilibrium shadow price λ* (marginal return per additional unit of capital).
    pub shadow_price: Decimal,
    /// List of per-opportunity allocations.
    pub allocations: Vec<AllocatedPosition>,
}

/// Solves constrained multi-pool capital allocation across heterogeneous AMM curves.
pub struct PortfolioSizer {
    /// Total available capital budget (e.g. flash loan size or wallet limit).
    pub total_capital_budget: Decimal,
    /// Convergence tolerance for capital budget bisection.
    pub tolerance: Decimal,
}

impl PortfolioSizer {
    /// Constructs a new `PortfolioSizer`.
    pub fn new(total_capital_budget: Decimal) -> Result<Self, SizingError> {
        if total_capital_budget <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("total_capital_budget must be > 0, got {total_capital_budget}"),
            });
        }
        Ok(Self {
            total_capital_budget,
            tolerance: dec!(0.0001),
        })
    }

    /// Evaluates allocations for candidate shadow price `lambda`.
    fn evaluate_lambda(
        &self,
        opportunities: &[PortfolioOpportunity],
        lambda: Decimal,
    ) -> (Decimal, Vec<AllocatedPosition>) {
        let mut total_capital = Decimal::ZERO;
        let mut positions = Vec::with_capacity(opportunities.len());

        for opp in opportunities {
            let effective_price = opp.reference_price + lambda;
            match opp.pool.optimal_size(effective_price, opp.constraints.clone()) {
                Ok(res) if res.optimal_delta > Decimal::ZERO => {
                    total_capital += res.optimal_delta;
                    positions.push(AllocatedPosition {
                        id: opp.id.clone(),
                        allocated_size: res.optimal_delta,
                        expected_profit: res.expected_profit,
                        guarantee_tier: res.guarantee_tier,
                    });
                }
                _ => {
                    positions.push(AllocatedPosition {
                        id: opp.id.clone(),
                        allocated_size: Decimal::ZERO,
                        expected_profit: Decimal::ZERO,
                        guarantee_tier: GuaranteeTier::ProvenOptimal,
                    });
                }
            }
        }

        (total_capital, positions)
    }

    /// Optimizes capital allocation across the provided opportunities.
    pub fn optimize(
        &self,
        opportunities: &[PortfolioOpportunity],
    ) -> Result<PortfolioResult, SizingError> {
        if opportunities.is_empty() {
            return Ok(PortfolioResult {
                total_capital_used: Decimal::ZERO,
                total_expected_profit: Decimal::ZERO,
                shadow_price: Decimal::ZERO,
                allocations: Vec::new(),
            });
        }

        // 1. Evaluate unconstrained optimum at lambda = 0
        let (unconstrained_capital, unconstrained_positions) =
            self.evaluate_lambda(opportunities, Decimal::ZERO);

        // If total unconstrained capital is within budget, budget does not bind!
        if unconstrained_capital <= self.total_capital_budget {
            let total_profit: Decimal = unconstrained_positions
                .iter()
                .map(|p| p.expected_profit)
                .sum();
            return Ok(PortfolioResult {
                total_capital_used: unconstrained_capital,
                total_expected_profit: total_profit,
                shadow_price: Decimal::ZERO,
                allocations: unconstrained_positions,
            });
        }

        // 2. Budget binds: bracket upper lambda
        let mut lo_lambda = Decimal::ZERO;
        let mut hi_lambda = dec!(0.10);

        for _ in 0..16 {
            let (cap, _) = self.evaluate_lambda(opportunities, hi_lambda);
            if cap <= self.total_capital_budget {
                break;
            }
            hi_lambda *= dec!(2);
        }

        // 3. Bisect lambda to match total_capital_budget
        let mut best_lambda = hi_lambda;
        let mut best_positions = Vec::new();
        let mut best_capital = Decimal::ZERO;

        for _ in 0..48 {
            let mid_lambda = (lo_lambda + hi_lambda) / dec!(2);
            let (cap, positions) = self.evaluate_lambda(opportunities, mid_lambda);

            best_lambda = mid_lambda;
            best_capital = cap;
            best_positions = positions;

            if (cap - self.total_capital_budget).abs() <= self.tolerance * self.total_capital_budget
            {
                break;
            }

            if cap > self.total_capital_budget {
                lo_lambda = mid_lambda;
            } else {
                hi_lambda = mid_lambda;
            }
        }

        // Ensure total capital never strictly exceeds the hard budget limit
        if best_capital > self.total_capital_budget && best_capital > Decimal::ZERO {
            let scale = self.total_capital_budget / best_capital;
            for pos in &mut best_positions {
                pos.allocated_size *= scale;
            }
            best_capital = best_positions.iter().map(|p| p.allocated_size).sum();
        }

        let total_profit: Decimal = best_positions.iter().map(|p| p.expected_profit).sum();

        Ok(PortfolioResult {
            total_capital_used: best_capital,
            total_expected_profit: total_profit,
            shadow_price: best_lambda,
            allocations: best_positions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sizing_core::curves::Cpmm;

    #[test]
    fn unconstrained_budget_gives_full_allocation() {
        let pool1 = Cpmm::new(dec!(1_000_000), dec!(1_000_000), dec!(0.997)).unwrap();
        let pool2 = Cpmm::new(dec!(2_000_000), dec!(2_000_000), dec!(0.997)).unwrap();

        let opps = vec![
            PortfolioOpportunity {
                id: "pool1".to_string(),
                pool: Box::new(pool1),
                reference_price: dec!(0.95),
                constraints: SizingConstraints {
                    fixed_cost: dec!(10),
                    max_size: None,
                },
            },
            PortfolioOpportunity {
                id: "pool2".to_string(),
                pool: Box::new(pool2),
                reference_price: dec!(0.95),
                constraints: SizingConstraints {
                    fixed_cost: dec!(10),
                    max_size: None,
                },
            },
        ];

        let sizer = PortfolioSizer::new(dec!(10_000_000)).unwrap();
        let result = sizer.optimize(&opps).unwrap();

        assert_eq!(result.shadow_price, Decimal::ZERO);
        assert!(result.total_capital_used > Decimal::ZERO);
        assert!(result.total_expected_profit > dec!(20));
    }

    #[test]
    fn constrained_budget_allocates_up_to_cap() {
        let pool1 = Cpmm::new(dec!(1_000_000), dec!(1_000_000), dec!(0.997)).unwrap();
        let pool2 = Cpmm::new(dec!(2_000_000), dec!(2_000_000), dec!(0.997)).unwrap();

        let opps = vec![
            PortfolioOpportunity {
                id: "pool1".to_string(),
                pool: Box::new(pool1),
                reference_price: dec!(0.95),
                constraints: SizingConstraints {
                    fixed_cost: dec!(10),
                    max_size: None,
                },
            },
            PortfolioOpportunity {
                id: "pool2".to_string(),
                pool: Box::new(pool2),
                reference_price: dec!(0.95),
                constraints: SizingConstraints {
                    fixed_cost: dec!(10),
                    max_size: None,
                },
            },
        ];

        let cap_limit = dec!(20_000);
        let sizer = PortfolioSizer::new(cap_limit).unwrap();
        let result = sizer.optimize(&opps).unwrap();

        assert!(result.shadow_price > Decimal::ZERO);
        // Total capital allocated should be very close to 20,000 and <= 20,000 + tolerance
        assert!(result.total_capital_used <= cap_limit + dec!(1));
        assert!(result.total_capital_used >= cap_limit - dec!(50));
    }
}
