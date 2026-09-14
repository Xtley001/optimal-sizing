//! Constant-product closed-form sizing. Struct/signatures per
//! `docs/API.md § curves/cpmm.rs`. Derivation and worked example in
//! `docs/whitepaper.md § 5.1`.

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;

use crate::error::SizingError;
use crate::profit::NetProfit;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

// bisection_fallback's stopping criterion is not named anywhere in
// API.md/whitepaper.md (unlike StableSwap's NEWTON_TOLERANCE or PMM's
// GOLDEN_SECTION_TOLERANCE). DECISION MADE (per this project's stated decision-logging discipline, logged in
// session handoff for Session 2): reuse NEWTON_TOLERANCE rather than invent
// a new constant. This needs to land in API.md as a documented note on
// bisection_fallback.
use crate::curves::stableswap::NEWTON_TOLERANCE;

/// A constant-product (`x * y = k`) pool with a fixed fee retention.
/// Sizing against this curve has a closed-form, provably optimal solution
/// — see `whitepaper.md § 5.1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Cpmm {
    /// Input-asset reserve.
    pub x: Decimal,
    /// Output-asset reserve.
    pub y: Decimal,
    /// Fee retention, e.g. `dec!(0.997)` for a 30bps fee.
    pub fee_retention: Decimal,
}

impl Cpmm {
    /// Constructs a new Cpmm. Returns SizingError::InvalidReserves if
    /// x <= 0, y <= 0, or fee_retention is not in (0, 1].
    pub fn new(x: Decimal, y: Decimal, fee_retention: Decimal) -> Result<Self, SizingError> {
        if x <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("x must be > 0, got {x}"),
            });
        }
        if y <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("y must be > 0, got {y}"),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }
        Ok(Self {
            x,
            y,
            fee_retention,
        })
    }

    /// Validation-only helper: computes the optimal size via bisection
    /// search instead of the closed form, for use in the proptest suite
    /// to confirm the two methods agree. Not part of the public sizing
    /// path — SizingAlgorithm::optimal_size always uses the closed form.
    ///
    /// Stopping criterion: bisects until the bracket width is within
    /// NEWTON_TOLERANCE (1e-6 relative) of the upper bracket bound. See
    /// the module-level DECISION MADE note above for why this constant
    /// (rather than a new one) is reused here.
    pub fn bisection_fallback(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError> {
        if reference_price <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("reference_price must be > 0, got {reference_price}"),
            });
        }

        let marginal_price = self.fee_retention * self.y / self.x;
        if marginal_price <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        // Profit'(Δx) = f*y*x / (x + f*Δx)^2 - P, strictly decreasing on
        // Δx >= 0 by equation (4) in whitepaper.md § 5.1 (strict concavity).
        let derivative = |delta: Decimal| -> Decimal {
            let denom = self.x + self.fee_retention * delta;
            (self.fee_retention * self.y * self.x) / (denom * denom) - reference_price
        };

        let mut lo = Decimal::ZERO;
        let mut hi = Decimal::ONE;
        // Doubling search for an upper bracket where the derivative has
        // gone negative. Guaranteed to terminate: derivative(0) > 0 (the
        // existence condition just checked) and derivative -> -P < 0 as
        // Δx -> infinity, since Profit' is strictly decreasing. The
        // iteration cap below is a defensive circuit breaker against a
        // runaway loop, not a precision parameter.
        let mut doubling_guard = 0u32;
        while derivative(hi) > Decimal::ZERO {
            hi *= Decimal::TWO;
            doubling_guard += 1;
            if doubling_guard > 10_000 {
                return Err(SizingError::InvalidReserves {
                    detail: "bisection_fallback failed to bracket a root".to_string(),
                });
            }
        }

        while hi - lo > NEWTON_TOLERANCE * hi {
            let mid = (lo + hi) / Decimal::TWO;
            if derivative(mid) > Decimal::ZERO {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let unconstrained_delta = (lo + hi) / Decimal::TWO;

        let (_, optimal_delta) =
            NetProfit::restrict_domain(&constraints, (Decimal::ZERO, unconstrained_delta))?;

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

impl PricingCurve for Cpmm {
    /// quote(Δx) = y * f * Δx / (x + f * Δx), per whitepaper.md § 5.1 eq. (1).
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        let denom = self.x + self.fee_retention * delta_in;
        if denom <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("x + fee_retention * delta_in must be > 0, got {denom}"),
            });
        }
        Ok(self.y * self.fee_retention * delta_in / denom)
    }
}

impl SizingAlgorithm for Cpmm {
    /// Closed-form optimum per whitepaper.md § 5.1 eq. (5):
    /// Δx* = (sqrt(x * y * f / P) - x) / f
    ///
    /// fixed_cost/capacity handling per whitepaper.md § 5.4 (Session 5):
    /// restrict_domain clips the domain to max_size; concavity (§ 5.1)
    /// guarantees that when max_size binds below the unconstrained
    /// optimum, the constrained optimum is exactly max_size (profit is
    /// still increasing there). The fixed_cost threshold is then checked
    /// separately here, per whitepaper.md § 5.4's explicit wording — if
    /// net profit at the resulting delta doesn't clear fixed_cost,
    /// SizingError::NoProfitableSize is returned instead of a result.
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

        // Existence condition per whitepaper.md § 5.1: a profitable trade
        // exists only if the pool's marginal price at zero size exceeds P.
        let marginal_price = self.fee_retention * self.y / self.x;
        if marginal_price <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let radicand = self.x * self.y * self.fee_retention / reference_price;
        let sqrt_radicand = radicand
            .sqrt()
            .ok_or_else(|| SizingError::InvalidReserves {
                detail: format!("failed to compute sqrt of radicand {radicand}"),
            })?;
        let unconstrained_delta = (sqrt_radicand - self.x) / self.fee_retention;

        let (_, optimal_delta) =
            NetProfit::restrict_domain(&constraints, (Decimal::ZERO, unconstrained_delta))?;

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
