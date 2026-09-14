//! `Leg`, `Route`. Multi-hop heterogeneous route composition across arbitrary curve families.
//! See `docs/ROUTING_SPEC.md` and `docs/ARCHITECTURE.md § sizing-router`.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use sizing_core::curves::{
    BalancerWeightedPool, ConcentratedLiquidity, Cpmm, CurveCryptoSwap, DodoPmm, Pmm, StableSwap,
};
use sizing_core::error::SizingError;
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints, SizingResult};

/// An individual hop in a multi-hop trading route across any supported curve family.
#[derive(Debug, Clone)]
pub enum Leg {
    /// Constant-product automated market maker (Uniswap v2).
    Cpmm(Cpmm),
    /// Stableswap curve for correlated assets (Curve v1).
    StableSwap(StableSwap),
    /// Proactive Market Maker (WooFi style).
    Pmm(Pmm),
    /// Concentrated liquidity position (Uniswap v3 / v4).
    ConcentratedLiquidity(ConcentratedLiquidity),
    /// Balancer multi-asset weighted constant-value pool.
    BalancerWeighted(BalancerWeightedPool),
    /// DODO Proactive Market Maker.
    DodoPmm(DodoPmm),
    /// Curve CryptoSwap dynamic-invariant pool (Curve v2).
    CurveCryptoSwap(CurveCryptoSwap),
}

impl Leg {
    /// Post-fee output for input amount `delta_in` through this specific leg.
    pub fn quote(&self, delta_in: Decimal) -> Result<Decimal, SizingError> {
        match self {
            Leg::Cpmm(c) => c.quote(delta_in),
            Leg::StableSwap(s) => s.quote(delta_in),
            Leg::Pmm(p) => p.quote(delta_in),
            Leg::ConcentratedLiquidity(cl) => cl.quote(delta_in),
            Leg::BalancerWeighted(b) => b.quote(delta_in),
            Leg::DodoPmm(d) => d.quote(delta_in),
            Leg::CurveCryptoSwap(cs) => cs.quote(delta_in),
        }
    }

    /// Diagnostic sample of this leg's own guarantee tier.
    pub fn diagnostic_tier(&self) -> Result<GuaranteeTier, SizingError> {
        let epsilon = dec!(0.000001);
        let probe = self.quote(epsilon)?;
        if probe <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: "diagnostic_tier probe quote was <= 0".to_string(),
            });
        }
        let marginal_price = probe / epsilon;
        let synthetic_reference_price = marginal_price * dec!(0.999);
        let constraints = SizingConstraints {
            fixed_cost: Decimal::ZERO,
            max_size: None,
        };
        let result = match self {
            Leg::Cpmm(c) => c.optimal_size(synthetic_reference_price, constraints),
            Leg::StableSwap(s) => s.optimal_size(synthetic_reference_price, constraints),
            Leg::Pmm(p) => p.optimal_size(synthetic_reference_price, constraints),
            Leg::ConcentratedLiquidity(cl) => {
                cl.optimal_size(synthetic_reference_price, constraints)
            }
            Leg::BalancerWeighted(b) => b.optimal_size(synthetic_reference_price, constraints),
            Leg::DodoPmm(d) => d.optimal_size(synthetic_reference_price, constraints),
            Leg::CurveCryptoSwap(cs) => cs.optimal_size(synthetic_reference_price, constraints),
        }?;
        Ok(result.guarantee_tier)
    }
}

/// Golden-ratio constant from sizing_core::curves::pmm.
use sizing_core::curves::pmm::{GOLDEN_RATIO_INV, GOLDEN_SECTION_TOLERANCE};

/// A multi-hop heterogeneous trading route ($N \ge 2$ legs).
#[derive(Debug, Clone)]
pub struct Route {
    /// Sequential hops where output of leg $i$ feeds input of leg $i+1$.
    pub legs: Vec<Leg>,
}

impl Route {
    /// Constructs a 2-hop route from two legs (backward compatible).
    pub fn new(leg1: Leg, leg2: Leg) -> Self {
        Self {
            legs: vec![leg1, leg2],
        }
    }

    /// Constructs an N-hop route from a vector of legs ($N \ge 2$).
    pub fn new_multi_hop(legs: Vec<Leg>) -> Result<Self, SizingError> {
        if legs.len() < 2 {
            return Err(SizingError::InvalidReserves {
                detail: format!("route must contain at least 2 legs, got {}", legs.len()),
            });
        }
        Ok(Self { legs })
    }

    /// First leg of the route.
    pub fn leg1(&self) -> &Leg {
        &self.legs[0]
    }

    /// Second leg of the route.
    pub fn leg2(&self) -> &Leg {
        &self.legs[1]
    }

    /// Total composite output for `delta_in` traversing all legs in sequence.
    pub fn route_output(&self, delta_in: Decimal) -> Option<Decimal> {
        let mut current = delta_in;
        for leg in &self.legs {
            current = leg.quote(current).ok()?;
        }
        Some(current)
    }

    /// Computes joint profit-maximizing route input size against whole-route `reference_price`.
    pub fn optimal_size(
        &self,
        reference_price: Decimal,
        constraints: SizingConstraints,
    ) -> Result<SizingResult, SizingError> {
        if reference_price <= Decimal::ZERO {
            return Err(SizingError::InvalidReserves {
                detail: format!("reference_price must be > 0, got {reference_price}"),
            });
        }

        let probe_epsilon = dec!(0.000001);
        let probe_output =
            self.route_output(probe_epsilon)
                .ok_or_else(|| SizingError::InvalidReserves {
                    detail: "route is infeasible at minimal probe size".to_string(),
                })?;
        let marginal_price = probe_output / probe_epsilon;
        if marginal_price <= reference_price {
            return Err(SizingError::NoArbitrageOpportunity);
        }

        let f = |x: Decimal| -> Decimal {
            match self.route_output(x) {
                Some(out) => out - reference_price * x,
                None => Decimal::MIN,
            }
        };

        let mut hi = Decimal::ONE;
        let mut guard = 0u32;
        while f(hi * Decimal::TWO) > f(hi) {
            hi *= Decimal::TWO;
            guard += 1;
            if guard > 60 {
                break;
            }
        }
        hi *= Decimal::TWO;
        if let Some(max_size) = constraints.max_size {
            hi = hi.min(max_size);
        }
        if hi <= Decimal::ZERO {
            return Err(SizingError::NoProfitableSize);
        }

        let (optimal_delta, iterations) =
            golden_section_maximize(f, Decimal::ZERO, hi, GOLDEN_SECTION_TOLERANCE);

        let route_out =
            self.route_output(optimal_delta)
                .ok_or_else(|| SizingError::InvalidReserves {
                    detail: "route became infeasible at the search's optimum".to_string(),
                })?;
        let gross_profit = route_out - reference_price * optimal_delta;
        let expected_profit = gross_profit - constraints.fixed_cost;
        if expected_profit <= Decimal::ZERO {
            return Err(SizingError::NoProfitableSize);
        }

        let tiers: Result<Vec<GuaranteeTier>, SizingError> =
            self.legs.iter().map(|l| l.diagnostic_tier()).collect();

        Ok(SizingResult {
            optimal_delta,
            expected_profit,
            guarantee_tier: GuaranteeTier::ComposedFrom(tiers?),
            iterations: Some(iterations),
        })
    }
}

/// Standard Golden-Section Search for unimodal function maximization.
fn golden_section_maximize<F: Fn(Decimal) -> Decimal>(
    f: F,
    lo: Decimal,
    hi: Decimal,
    tolerance: Decimal,
) -> (Decimal, u32) {
    let gr = GOLDEN_RATIO_INV;
    let mut a = lo;
    let mut b = hi;
    let mut c = b - gr * (b - a);
    let mut d = a + gr * (b - a);
    let mut fc = f(c);
    let mut fd = f(d);

    let mut iterations = 0u32;
    while (b - a) > tolerance * hi.max(Decimal::ONE) {
        if fc > fd {
            b = d;
            d = c;
            fd = fc;
            c = b - gr * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + gr * (b - a);
            fd = f(d);
        }
        iterations += 1;
        if iterations > 10_000 {
            break;
        }
    }
    ((a + b) / Decimal::TWO, iterations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sizing_core::curves::{BalancerWeightedPool, ConcentratedLiquidity};

    #[test]
    fn heterogeneous_3_hop_route_cpmm_balancer_clmm() {
        let leg1 = Leg::Cpmm(Cpmm::new(dec!(1_000_000), dec!(2_000_000), dec!(0.997)).unwrap());
        let leg2 = Leg::BalancerWeighted(
            BalancerWeightedPool::new(
                vec![dec!(2_000_000), dec!(1_500_000)],
                vec![dec!(0.5), dec!(0.5)],
                dec!(0.997),
                0,
                1,
            )
            .unwrap(),
        );
        let leg3 = Leg::ConcentratedLiquidity(
            ConcentratedLiquidity::new(
                dec!(5_000_000),
                dec!(0.5),
                dec!(2.0),
                dec!(1.0),
                dec!(0.997),
            )
            .unwrap(),
        );

        let route = Route::new_multi_hop(vec![leg1, leg2, leg3]).unwrap();
        let ref_price = dec!(1.0);

        let res = route
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
        if let GuaranteeTier::ComposedFrom(tiers) = res.guarantee_tier {
            assert_eq!(tiers.len(), 3);
        } else {
            panic!("expected ComposedFrom tier");
        }
    }
}
