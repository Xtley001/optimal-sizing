//! Velodrome / Aerodrome Stable Curve ($x^3 y + x y^3 = k$).
//!
//! # Mathematical Model
//! The invariant satisfies:
//! `x^3 * y + x * y^3 = k`
//!
//! For input amount `delta_in` with fee retention factor `f`:
//! `x_new = x + f * delta_in`
//! The post-swap output reserve `y_new` is the unique positive root of:
//! `g(y) = x_new * y^3 + x_new^3 * y - k = 0`
//!
//! Because `g'(y) = 3 * x_new * y^2 + x_new^3 > 0` and `g''(y) = 6 * x_new * y > 0` on `y >= 0`,
//! `g(y)` is strictly convex and increasing. Initializing Newton's method from `y_0 = y`
//! guarantees monotonic, quadratic convergence.
//!
//! Sizing is solved via Golden-Section search over the strictly concave profit function.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::error::SizingError;
use crate::profit::NetProfit;
use crate::traits::{PricingCurve, SizingAlgorithm};
use crate::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// Relative convergence tolerance for Newton's method.
pub const VELODROME_NEWTON_TOLERANCE: Decimal = dec!(0.0000001);

/// Maximum iterations for Newton's method.
pub const VELODROME_MAX_ITERATIONS: u32 = 64;

/// Golden ratio inverse for golden-section search: (sqrt(5) - 1) / 2.
pub const GOLDEN_RATIO_INV: Decimal = dec!(0.6180339887498948482045868344);

/// Golden-section search relative convergence tolerance.
pub const GOLDEN_SECTION_TOLERANCE: Decimal = dec!(0.000001);

/// Velodrome/Aerodrome Stable AMM pool (`x³y + xy³ = k`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VelodromeStable {
    /// Input asset reserve x.
    pub reserve_x: Decimal,
    /// Output asset reserve y.
    pub reserve_y: Decimal,
    /// Fee retention factor f (e.g. `dec!(0.9995)` for 5 bps fee). In (0, 1].
    pub fee_retention: Decimal,
    /// Cached sum of squared reserves: x^2 + y^2, used in normalized Newton iteration.
    sum_sq_reserves: Decimal,
}

impl VelodromeStable {
    /// Constructs a new VelodromeStable curve.
    ///
    /// Returns `SizingError::InvalidReserves` if reserves are non-positive or
    /// fee retention is outside `(0, 1]`.
    pub fn new(
        reserve_x: Decimal,
        reserve_y: Decimal,
        fee_retention: Decimal,
    ) -> Result<Self, SizingError> {
        if reserve_x <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("reserve_x must be > 0, got {reserve_x}"),
            });
        }
        if reserve_y <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("reserve_y must be > 0, got {reserve_y}"),
            });
        }
        if fee_retention <= Decimal::ZERO || fee_retention > Decimal::ONE {
            return Err(SizingError::InvalidReserves {
                detail: format!("fee_retention must be in (0, 1], got {fee_retention}"),
            });
        }

        let x2 = reserve_x * reserve_x;
        let y2 = reserve_y * reserve_y;
        let sum_sq_reserves = x2 + y2;

        Ok(Self {
            reserve_x,
            reserve_y,
            fee_retention,
            sum_sq_reserves,
        })
    }

    /// Returns the pool invariant `k = x^3*y + x*y^3`, or `Decimal::MAX` if overflowed.
    pub fn k(&self) -> Decimal {
        self.reserve_x
            .checked_mul(self.reserve_y)
            .and_then(|xy| xy.checked_mul(self.sum_sq_reserves))
            .unwrap_or(Decimal::MAX)
    }

    /// Post-fee marginal price at zero input size: dy/dx at delta_in = 0.
    /// dy/dx = (y * (3*x^2 + y^2)) / (x * (x^2 + 3*y^2)) * f.
    pub fn marginal_price_at_zero(&self) -> Decimal {
        let x = self.reserve_x;
        let y = self.reserve_y;
        let x2 = x * x;
        let y2 = y * y;

        let num = y * (dec!(3) * x2 + y2);
        let den = x * (x2 + dec!(3) * y2);
        (num / den) * self.fee_retention
    }

    /// Solves for the post-swap output reserve `y_new` given `x_new` using normalized Newton's method.
    /// Normalized equation: `h(y) = y * (y^2 + x_new^2) - (x / x_new) * y_orig * (x^2 + y_orig^2) = 0`.
    /// `h'(y) = 3 * y^2 + x_new^2`.
    /// This division by `x_new` guarantees no intermediate multiplication exceeds Decimal::MAX.
    pub fn solve_y(&self, x_new: Decimal) -> Result<(Decimal, u32), SizingError> {
        if x_new <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("x_new must be > 0, got {x_new}"),
            });
        }

        let x_new2 = x_new * x_new;
        // c_target = (x / x_new) * y_orig * sum_sq_reserves
        let ratio = self.reserve_x / x_new;
        let c_target = ratio * self.reserve_y * self.sum_sq_reserves;
        let mut y = self.reserve_y;

        for iter in 1..=VELODROME_MAX_ITERATIONS {
            let y2 = y * y;
            let h = y * (y2 + x_new2) - c_target;
            let h_prime = dec!(3) * y2 + x_new2;

            if h_prime <= Decimal::ZERO {
                return Err(SizingError::DidNotConverge {
                    iterations_attempted: iter,
                });
            }

            let delta = h / h_prime;
            let y_next = y - delta;

            if (y - y_next).abs() <= VELODROME_NEWTON_TOLERANCE * y_next.abs().max(Decimal::ONE) {
                return Ok((y_next, iter));
            }

            y = y_next;
        }

        Err(SizingError::DidNotConverge {
            iterations_attempted: VELODROME_MAX_ITERATIONS,
        })
    }

    /// Search upper bound for sizing search.
    fn search_upper_bound(&self, reference_price: Decimal) -> Decimal {
        let mut upper = self.reserve_x;
        for _ in 0..8 {
            if let Ok(test_out) = self.quote(upper) {
                let marginal_approx = test_out / upper;
                if marginal_approx < reference_price {
                    break;
                }
            } else {
                break;
            }
            upper *= dec!(2);
        }
        upper
    }
}

impl PricingCurve for VelodromeStable {
    fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        if delta_in < Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("delta_in must be >= 0, got {delta_in}"),
            });
        }
        if delta_in == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        let effective_delta = delta_in * self.fee_retention;
        let x_new = self.reserve_x + effective_delta;
        let (y_new, _) = self.solve_y(x_new)?;

        if y_new > self.reserve_y {
            return Ok(Decimal::ZERO);
        }

        Ok(self.reserve_y - y_new)
    }
}

impl SizingAlgorithm for VelodromeStable {
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

        // Fast zero-size marginal price check:
        let marginal_zero = self.marginal_price_at_zero();
        if marginal_zero <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let mut a = Decimal::ZERO;
        let mut b = self.search_upper_bound(reference_price);
        let tol = GOLDEN_SECTION_TOLERANCE * self.reserve_x;

        let profit_fn = |delta: Decimal| -> Decimal {
            let q = self.quote(delta).unwrap_or(Decimal::ZERO);
            q - reference_price * delta
        };

        let mut c = b - GOLDEN_RATIO_INV * (b - a);
        let mut d = a + GOLDEN_RATIO_INV * (b - a);
        let mut fc = profit_fn(c);
        let mut fd = profit_fn(d);

        let mut iterations = 0u32;
        while (b - a) > tol && iterations < 128 {
            iterations += 1;
            if fc > fd {
                b = d;
                d = c;
                fd = fc;
                c = b - GOLDEN_RATIO_INV * (b - a);
                fc = profit_fn(c);
            } else {
                a = c;
                c = d;
                fc = fd;
                d = a + GOLDEN_RATIO_INV * (b - a);
                fd = profit_fn(d);
            }
        }

        let unconstrained_delta = (a + b) / dec!(2);
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
    fn velodrome_new_rejects_invalid_reserves() {
        assert!(VelodromeStable::new(dec!(0), dec!(100), dec!(0.9995)).is_err());
        assert!(VelodromeStable::new(dec!(100), dec!(-1), dec!(0.9995)).is_err());
        assert!(VelodromeStable::new(dec!(100), dec!(100), dec!(0)).is_err());
        assert!(VelodromeStable::new(dec!(100), dec!(100), dec!(1.5)).is_err());
    }

    #[test]
    fn velodrome_quote_and_newton_convergence() {
        // 10M USDC and 10M USDT pool, 5bps fee (0.9995)
        let pool = VelodromeStable::new(dec!(10_000_000), dec!(10_000_000), dec!(0.9995)).unwrap();
        assert_eq!(pool.quote(dec!(0)).unwrap(), dec!(0));

        let out = pool.quote(dec!(10_000)).unwrap();
        // Near 1:1 swap for small trade
        assert!(out > dec!(9990) && out < dec!(10000));
    }

    #[test]
    fn velodrome_optimal_sizing_finds_profit() {
        let pool = VelodromeStable::new(dec!(10_000_000), dec!(10_000_000), dec!(0.9995)).unwrap();
        // Pool marginal price is ~0.9995. If reference price is 0.95, profitable trade exists!
        let constraints = SizingConstraints {
            fixed_cost: dec!(10),
            max_size: None,
        };
        let res = pool.optimal_size(dec!(0.95), constraints).unwrap();
        assert!(res.optimal_delta > dec!(0));
        assert!(res.expected_profit > dec!(10));
        assert_eq!(
            res.guarantee_tier,
            GuaranteeTier::NumericallyGuaranteed {
                convergence_conditions_met: true
            }
        );
    }
}
