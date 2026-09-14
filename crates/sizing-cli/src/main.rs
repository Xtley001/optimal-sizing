//! `sizing-cli` — CLI tool for optimal trade sizing across DEX curve families & multi-hop routes.
//! See `docs/ROUTING_SPEC.md § 3`.

use clap::{Parser, Subcommand};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::fs;

use sizing_core::curves::{
    BalancerWeightedPool, ConcentratedLiquidity, Cpmm, CurveCryptoSwap, DodoPmm, Pmm, StableSwap,
};
use sizing_core::error::SizingError;
use sizing_core::traits::SizingAlgorithm;
use sizing_core::types::{GuaranteeTier, SizingConstraints, SizingResult};
use sizing_router::{Leg, Route};

#[derive(Parser)]
#[command(
    name = "sizing-cli",
    about = "Quick one-off optimal trade sizing against sizing-core, no Rust project required."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LegSpec {
    Cpmm {
        x: Decimal,
        y: Decimal,
        fee: Decimal,
    },
    Stableswap {
        reserves: Vec<Decimal>,
        amplification: Decimal,
        fee: Decimal,
    },
    Pmm {
        base_reserve: Decimal,
        quote_reserve: Decimal,
        oracle_price: Decimal,
        k: Decimal,
    },
    Clmm {
        liquidity: Decimal,
        sqrt_price_lower: Decimal,
        sqrt_price_upper: Decimal,
        sqrt_price_current: Decimal,
        fee: Decimal,
    },
    Balancer {
        reserves: Vec<Decimal>,
        weights: Vec<Decimal>,
        fee: Decimal,
        #[serde(default)]
        input_index: usize,
        #[serde(default = "default_output_index")]
        output_index: usize,
    },
    Dodo {
        base_target: Decimal,
        quote_target: Decimal,
        base_reserve: Decimal,
        quote_reserve: Decimal,
        oracle_price: Decimal,
        k: Decimal,
        fee: Decimal,
        #[serde(default = "default_true")]
        is_sell_base: bool,
    },
    Cryptoswap {
        reserves: Vec<Decimal>,
        a: Decimal,
        gamma: Decimal,
        fee: Decimal,
        #[serde(default)]
        input_index: usize,
    },
}

fn default_output_index() -> usize {
    1
}

fn default_true() -> bool {
    true
}

impl LegSpec {
    fn to_leg(&self) -> Result<Leg, SizingError> {
        match self {
            LegSpec::Cpmm { x, y, fee } => Ok(Leg::Cpmm(Cpmm::new(*x, *y, *fee)?)),
            LegSpec::Stableswap {
                reserves,
                amplification,
                fee,
            } => Ok(Leg::StableSwap(StableSwap::new(
                reserves.clone(),
                *amplification,
                *fee,
            )?)),
            LegSpec::Pmm {
                base_reserve,
                quote_reserve,
                oracle_price,
                k,
            } => Ok(Leg::Pmm(Pmm::new(
                *base_reserve,
                *quote_reserve,
                *oracle_price,
                *k,
            )?)),
            LegSpec::Clmm {
                liquidity,
                sqrt_price_lower,
                sqrt_price_upper,
                sqrt_price_current,
                fee,
            } => Ok(Leg::ConcentratedLiquidity(ConcentratedLiquidity::new(
                *liquidity,
                *sqrt_price_lower,
                *sqrt_price_upper,
                *sqrt_price_current,
                *fee,
            )?)),
            LegSpec::Balancer {
                reserves,
                weights,
                fee,
                input_index,
                output_index,
            } => Ok(Leg::BalancerWeighted(BalancerWeightedPool::new(
                reserves.clone(),
                weights.clone(),
                *fee,
                *input_index,
                *output_index,
            )?)),
            LegSpec::Dodo {
                base_target,
                quote_target,
                base_reserve,
                quote_reserve,
                oracle_price,
                k,
                fee,
                is_sell_base,
            } => Ok(Leg::DodoPmm(DodoPmm::new(
                *base_target,
                *quote_target,
                *base_reserve,
                *quote_reserve,
                *oracle_price,
                *k,
                *fee,
                *is_sell_base,
            )?)),
            LegSpec::Cryptoswap {
                reserves,
                a,
                gamma,
                fee,
                input_index,
            } => {
                if reserves.len() != 2 {
                    return Err(SizingError::InvalidReserves {
                        detail: format!(
                            "CryptoSwap requires exactly 2 reserves, got {}",
                            reserves.len()
                        ),
                    });
                }
                Ok(Leg::CurveCryptoSwap(CurveCryptoSwap::new(
                    [reserves[0], reserves[1]],
                    *a,
                    *gamma,
                    *fee,
                    *input_index,
                )?))
            }
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Size a constant-product (x*y=k) pool.
    Cpmm {
        #[arg(long)]
        x: Decimal,
        #[arg(long)]
        y: Decimal,
        #[arg(long)]
        fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a StableSwap-invariant pool.
    Stableswap {
        #[arg(long, value_delimiter = ',')]
        reserves: Vec<Decimal>,
        #[arg(long)]
        amplification: Decimal,
        #[arg(long)]
        fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a WooFi-style PMM pool.
    Pmm {
        #[arg(long)]
        base_reserve: Decimal,
        #[arg(long)]
        quote_reserve: Decimal,
        #[arg(long)]
        oracle_price: Decimal,
        #[arg(long)]
        k: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a concentrated-liquidity pool / position.
    Clmm {
        #[arg(long)]
        liquidity: Decimal,
        #[arg(long)]
        sqrt_price_lower: Decimal,
        #[arg(long)]
        sqrt_price_upper: Decimal,
        #[arg(long)]
        sqrt_price_current: Decimal,
        #[arg(long)]
        fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a Balancer weighted constant-value pool.
    Balancer {
        #[arg(long, value_delimiter = ',')]
        reserves: Vec<Decimal>,
        #[arg(long, value_delimiter = ',')]
        weights: Vec<Decimal>,
        #[arg(long)]
        fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        input_index: usize,
        #[arg(long, default_value = "1")]
        output_index: usize,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a DODO Proactive Market Maker (PMM) pool.
    Dodo {
        #[arg(long)]
        base_reserve: Decimal,
        #[arg(long)]
        quote_reserve: Decimal,
        #[arg(long)]
        target_base: Option<Decimal>,
        #[arg(long)]
        target_quote: Option<Decimal>,
        #[arg(long)]
        oracle_price: Decimal,
        #[arg(long)]
        k: Decimal,
        #[arg(long, default_value = "0.997")]
        fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "true")]
        is_sell_base: bool,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a Curve CryptoSwap v2 pool.
    Cryptoswap {
        #[arg(long, value_delimiter = ',')]
        reserves: Vec<Decimal>,
        #[arg(long)]
        a: Decimal,
        #[arg(long)]
        gamma: Decimal,
        #[arg(long, default_value = "0.996")]
        fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        input_index: usize,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size an arbitrary multi-hop heterogeneous route specified via JSON string or file.
    Route {
        #[arg(long)]
        legs_json: Option<String>,
        #[arg(long)]
        spec_file: Option<String>,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a single concentrated-liquidity tick range (alias).
    ConcentratedLiquidity {
        #[arg(long)]
        liquidity: Decimal,
        #[arg(long)]
        sqrt_price_lower: Decimal,
        #[arg(long)]
        sqrt_price_upper: Decimal,
        #[arg(long)]
        sqrt_price_current: Decimal,
        #[arg(long)]
        fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
    /// Size a 2-hop CPMM route (alias).
    Route2HopCpmm {
        #[arg(long)]
        leg1_x: Decimal,
        #[arg(long)]
        leg1_y: Decimal,
        #[arg(long)]
        leg1_fee: Decimal,
        #[arg(long)]
        leg2_x: Decimal,
        #[arg(long)]
        leg2_y: Decimal,
        #[arg(long)]
        leg2_fee: Decimal,
        #[arg(long)]
        price: Decimal,
        #[arg(long, default_value = "0")]
        fixed_cost: Decimal,
        #[arg(long)]
        max_size: Option<Decimal>,
    },
}

fn tier_line(tier: &GuaranteeTier) -> String {
    match tier {
        GuaranteeTier::ProvenOptimal => "ProvenOptimal".to_string(),
        GuaranteeTier::NumericallyGuaranteed {
            convergence_conditions_met,
        } => {
            format!("NumericallyGuaranteed {{ convergence_conditions_met: {convergence_conditions_met} }}")
        }
        GuaranteeTier::EmpiricallyValidated {
            unimodality_confirmed,
        } => {
            format!("EmpiricallyValidated {{ unimodality_confirmed: {unimodality_confirmed} }}")
        }
        GuaranteeTier::ComposedFrom(legs) => {
            let inner: Vec<String> = legs.iter().map(tier_line).collect();
            format!("ComposedFrom([{}])", inner.join(", "))
        }
    }
}

fn print_result(result: &SizingResult) {
    println!("optimal_delta:    {}", result.optimal_delta);
    println!("expected_profit:  {}", result.expected_profit);
    println!("guarantee_tier:   {}", tier_line(&result.guarantee_tier));
    match result.iterations {
        Some(n) => println!("iterations:       {n}"),
        None => println!("iterations:       n/a (closed form)"),
    }
}

fn print_error(e: &SizingError) -> ! {
    eprintln!("sizing-cli: {e:?}");
    std::process::exit(1);
}

fn main() {
    let cli = Cli::parse();

    let outcome: Result<SizingResult, SizingError> = match cli.command {
        Command::Cpmm {
            x,
            y,
            fee,
            price,
            fixed_cost,
            max_size,
        } => Cpmm::new(x, y, fee).and_then(|pool| {
            pool.optimal_size(
                price,
                SizingConstraints {
                    fixed_cost,
                    max_size,
                },
            )
        }),
        Command::Stableswap {
            reserves,
            amplification,
            fee,
            price,
            fixed_cost,
            max_size,
        } => StableSwap::new(reserves, amplification, fee).and_then(|pool| {
            pool.optimal_size(
                price,
                SizingConstraints {
                    fixed_cost,
                    max_size,
                },
            )
        }),
        Command::Pmm {
            base_reserve,
            quote_reserve,
            oracle_price,
            k,
            price,
            fixed_cost,
            max_size,
        } => Pmm::new(base_reserve, quote_reserve, oracle_price, k).and_then(|pool| {
            pool.optimal_size(
                price,
                SizingConstraints {
                    fixed_cost,
                    max_size,
                },
            )
        }),
        Command::Clmm {
            liquidity,
            sqrt_price_lower,
            sqrt_price_upper,
            sqrt_price_current,
            fee,
            price,
            fixed_cost,
            max_size,
        }
        | Command::ConcentratedLiquidity {
            liquidity,
            sqrt_price_lower,
            sqrt_price_upper,
            sqrt_price_current,
            fee,
            price,
            fixed_cost,
            max_size,
        } => ConcentratedLiquidity::new(
            liquidity,
            sqrt_price_lower,
            sqrt_price_upper,
            sqrt_price_current,
            fee,
        )
        .and_then(|pool| {
            pool.optimal_size(
                price,
                SizingConstraints {
                    fixed_cost,
                    max_size,
                },
            )
        }),
        Command::Balancer {
            reserves,
            weights,
            fee,
            price,
            input_index,
            output_index,
            fixed_cost,
            max_size,
        } => BalancerWeightedPool::new(reserves, weights, fee, input_index, output_index).and_then(
            |pool| {
                pool.optimal_size(
                    price,
                    SizingConstraints {
                        fixed_cost,
                        max_size,
                    },
                )
            },
        ),
        Command::Dodo {
            base_reserve,
            quote_reserve,
            target_base,
            target_quote,
            oracle_price,
            k,
            fee,
            price,
            is_sell_base,
            fixed_cost,
            max_size,
        } => {
            let b0 = target_base.unwrap_or(base_reserve);
            let q0 = target_quote.unwrap_or(quote_reserve);
            DodoPmm::new(
                b0,
                q0,
                base_reserve,
                quote_reserve,
                oracle_price,
                k,
                fee,
                is_sell_base,
            )
            .and_then(|pool| {
                pool.optimal_size(
                    price,
                    SizingConstraints {
                        fixed_cost,
                        max_size,
                    },
                )
            })
        }
        Command::Cryptoswap {
            reserves,
            a,
            gamma,
            fee,
            price,
            input_index,
            fixed_cost,
            max_size,
        } => {
            if reserves.len() != 2 {
                Err(SizingError::InvalidReserves {
                    detail: format!("CryptoSwap requires 2 reserves, got {}", reserves.len()),
                })
            } else {
                CurveCryptoSwap::new([reserves[0], reserves[1]], a, gamma, fee, input_index)
                    .and_then(|pool| {
                        pool.optimal_size(
                            price,
                            SizingConstraints {
                                fixed_cost,
                                max_size,
                            },
                        )
                    })
            }
        }
        Command::Route {
            legs_json,
            spec_file,
            price,
            fixed_cost,
            max_size,
        } => {
            let json_str = if let Some(j) = legs_json {
                j
            } else if let Some(path) = spec_file {
                match fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to read spec file {path}: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("Either --legs-json or --spec-file must be provided");
                std::process::exit(1);
            };

            let specs: Vec<LegSpec> = match serde_json::from_str(&json_str) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to parse legs JSON: {e}");
                    std::process::exit(1);
                }
            };

            let legs: Vec<Leg> = match specs
                .into_iter()
                .map(|s| s.to_leg())
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(l) => l,
                Err(e) => print_error(&e),
            };

            Route::new_multi_hop(legs).and_then(|route| {
                route.optimal_size(
                    price,
                    SizingConstraints {
                        fixed_cost,
                        max_size,
                    },
                )
            })
        }
        Command::Route2HopCpmm {
            leg1_x,
            leg1_y,
            leg1_fee,
            leg2_x,
            leg2_y,
            leg2_fee,
            price,
            fixed_cost,
            max_size,
        } => match (
            Cpmm::new(leg1_x, leg1_y, leg1_fee),
            Cpmm::new(leg2_x, leg2_y, leg2_fee),
        ) {
            (Ok(leg1), Ok(leg2)) => {
                let route = Route::new(Leg::Cpmm(leg1), Leg::Cpmm(leg2));
                route.optimal_size(
                    price,
                    SizingConstraints {
                        fixed_cost,
                        max_size,
                    },
                )
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
        },
    };

    match outcome {
        Ok(result) => print_result(&result),
        Err(e) => print_error(&e),
    }
}
