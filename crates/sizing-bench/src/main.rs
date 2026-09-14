//! Grid-search comparison harness, writes `docs/benchmark-report.md`.
//! Per `docs/ARCHITECTURE.md § sizing-bench`: benchmarks `sizing-core`
//! against a naive grid-search baseline across all 7 curve families.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sizing_core::curves::{
    BalancerWeightedPool, ConcentratedLiquidity, Cpmm, CurveCryptoSwap, DodoPmm, Pmm, StableSwap,
};
use sizing_core::{PricingCurve, SizingAlgorithm, SizingConstraints};
use std::fmt::Write as _;
use std::time::Instant;

const GRID_DENSITIES: [u32; 4] = [10, 100, 1_000, 10_000];
const ACCURACY_TOLERANCE: Decimal = dec!(0.0001); // 1e-4, per TESTING.md

/// Naive grid-search baseline: evaluate Profit(Δ) at `n` evenly spaced points.
fn grid_search<F>(upper: Decimal, n: u32, profit_at: F) -> (Decimal, Decimal)
where
    F: Fn(Decimal) -> Option<Decimal>,
{
    let mut best_delta = Decimal::ZERO;
    let mut best_profit = Decimal::MIN;
    for i in 0..=n {
        let delta = upper * Decimal::from(i) / Decimal::from(n);
        if let Some(profit) = profit_at(delta) {
            if profit > best_profit {
                best_profit = profit;
                best_delta = delta;
            }
        }
    }
    (best_delta, best_profit)
}

struct CurveBenchResult {
    name: &'static str,
    guaranteed_method: &'static str,
    guaranteed_delta: Decimal,
    guaranteed_iterations: String,
    guaranteed_wall_clock_micros: u128,
    grid_rows: Vec<GridRow>,
    min_converging_n: Option<u32>,
}

struct GridRow {
    n: u32,
    delta: Decimal,
    wall_clock_micros: u128,
    relative_diff: Decimal,
}

fn bench_cpmm() -> CurveBenchResult {
    let pool = Cpmm::new(dec!(1_200_000), dec!(800_000), dec!(0.997)).unwrap();
    let reference_price = dec!(0.60);
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };

    let start = Instant::now();
    let guaranteed = pool.optimal_size(reference_price, constraints).unwrap();
    let guaranteed_wall_clock_micros = start.elapsed().as_micros();

    let upper = guaranteed.optimal_delta * Decimal::new(219, 2);
    let profit_at = |delta: Decimal| pool.quote(delta).ok().map(|q| q - reference_price * delta);

    run_grid_comparison(
        "Cpmm",
        "Closed form (eq. 5)",
        guaranteed.optimal_delta,
        "N/A (O(1))".to_string(),
        guaranteed_wall_clock_micros,
        upper,
        profit_at,
    )
}

fn bench_stableswap() -> CurveBenchResult {
    let pool = StableSwap::new(
        vec![dec!(15_000_000), dec!(8_000_000), dec!(9_500_000)],
        dec!(100),
        dec!(0.997),
    )
    .unwrap();
    let reference_price = dec!(0.5);
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };

    let start = Instant::now();
    let guaranteed = pool.optimal_size(reference_price, constraints).unwrap();
    let guaranteed_wall_clock_micros = start.elapsed().as_micros();

    let upper = guaranteed.optimal_delta * Decimal::new(219, 2);
    let profit_at = |delta: Decimal| pool.quote(delta).ok().map(|q| q - reference_price * delta);

    run_grid_comparison(
        "StableSwap",
        "Newton's method",
        guaranteed.optimal_delta,
        guaranteed
            .iterations
            .map(|i| i.to_string())
            .unwrap_or_else(|| "N/A".to_string()),
        guaranteed_wall_clock_micros,
        upper,
        profit_at,
    )
}

fn bench_pmm() -> CurveBenchResult {
    let pool = Pmm::new(
        dec!(1_000_000),
        dec!(2_000_000_000),
        dec!(2000),
        dec!(0.0000001),
    )
    .unwrap();
    let reference_price = dec!(1900);
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };

    let start = Instant::now();
    let guaranteed = pool.optimal_size(reference_price, constraints).unwrap();
    let guaranteed_wall_clock_micros = start.elapsed().as_micros();

    let mut upper = guaranteed.optimal_delta * Decimal::new(219, 2);
    let pole = Decimal::ONE / (pool.k * pool.oracle_price);
    let pole_margin = pole * dec!(0.999);
    if pole_margin < upper {
        upper = pole_margin;
    }

    let profit_at = |delta: Decimal| pool.quote(delta).ok().map(|q| q - reference_price * delta);

    run_grid_comparison(
        "Pmm",
        "Golden-section search",
        guaranteed.optimal_delta,
        guaranteed
            .iterations
            .map(|i| i.to_string())
            .unwrap_or_else(|| "N/A".to_string()),
        guaranteed_wall_clock_micros,
        upper,
        profit_at,
    )
}

fn bench_clmm() -> CurveBenchResult {
    let pool = ConcentratedLiquidity::new(
        dec!(5_000_000),
        dec!(0.5),
        dec!(2.0),
        dec!(1.0),
        dec!(0.997),
    )
    .unwrap();
    let reference_price = dec!(0.90);
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };

    let start = Instant::now();
    let guaranteed = pool.optimal_size(reference_price, constraints).unwrap();
    let guaranteed_wall_clock_micros = start.elapsed().as_micros();

    let upper = guaranteed.optimal_delta * Decimal::new(219, 2);
    let profit_at = |delta: Decimal| pool.quote(delta).ok().map(|q| q - reference_price * delta);

    run_grid_comparison(
        "ConcentratedLiquidity",
        "Piecewise closed form",
        guaranteed.optimal_delta,
        "N/A (O(1))".to_string(),
        guaranteed_wall_clock_micros,
        upper,
        profit_at,
    )
}

fn bench_balancer() -> CurveBenchResult {
    let pool = BalancerWeightedPool::new(
        vec![dec!(1_000_000), dec!(2_000_000)],
        vec![dec!(0.8), dec!(0.2)],
        dec!(0.997),
        0,
        1,
    )
    .unwrap();
    let reference_price = dec!(0.60);
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };

    let start = Instant::now();
    let guaranteed = pool.optimal_size(reference_price, constraints).unwrap();
    let guaranteed_wall_clock_micros = start.elapsed().as_micros();

    let upper = guaranteed.optimal_delta * Decimal::new(219, 2);
    let profit_at = |delta: Decimal| pool.quote(delta).ok().map(|q| q - reference_price * delta);

    run_grid_comparison(
        "BalancerWeighted",
        "Closed form power-law",
        guaranteed.optimal_delta,
        "N/A (O(1))".to_string(),
        guaranteed_wall_clock_micros,
        upper,
        profit_at,
    )
}

fn bench_dodo() -> CurveBenchResult {
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
    let reference_price = dec!(1.50);
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };

    let start = Instant::now();
    let guaranteed = pool.optimal_size(reference_price, constraints).unwrap();
    let guaranteed_wall_clock_micros = start.elapsed().as_micros();

    let upper = guaranteed.optimal_delta * Decimal::new(219, 2);
    let profit_at = |delta: Decimal| pool.quote(delta).ok().map(|q| q - reference_price * delta);

    run_grid_comparison(
        "DodoPmm",
        "Golden-section search",
        guaranteed.optimal_delta,
        guaranteed
            .iterations
            .map(|i| i.to_string())
            .unwrap_or_else(|| "N/A".to_string()),
        guaranteed_wall_clock_micros,
        upper,
        profit_at,
    )
}

fn bench_cryptoswap() -> CurveBenchResult {
    let pool = CurveCryptoSwap::new(
        [dec!(1_000_000), dec!(1_000_000)],
        dec!(100),
        dec!(0.0001),
        dec!(0.997),
        0,
    )
    .unwrap();
    let reference_price = dec!(0.90);
    let constraints = SizingConstraints {
        fixed_cost: dec!(0),
        max_size: None,
    };

    let start = Instant::now();
    let guaranteed = pool.optimal_size(reference_price, constraints).unwrap();
    let guaranteed_wall_clock_micros = start.elapsed().as_micros();

    let upper = guaranteed.optimal_delta * Decimal::new(219, 2);
    let profit_at = |delta: Decimal| pool.quote(delta).ok().map(|q| q - reference_price * delta);

    run_grid_comparison(
        "CurveCryptoSwap",
        "Dynamic invariant solver",
        guaranteed.optimal_delta,
        guaranteed
            .iterations
            .map(|i| i.to_string())
            .unwrap_or_else(|| "N/A".to_string()),
        guaranteed_wall_clock_micros,
        upper,
        profit_at,
    )
}

fn run_grid_comparison<F>(
    name: &'static str,
    guaranteed_method: &'static str,
    guaranteed_delta: Decimal,
    guaranteed_iterations: String,
    guaranteed_wall_clock_micros: u128,
    upper: Decimal,
    profit_at: F,
) -> CurveBenchResult
where
    F: Fn(Decimal) -> Option<Decimal>,
{
    let mut grid_rows = Vec::new();
    let mut min_converging_n = None;

    for &n in &GRID_DENSITIES {
        let start = Instant::now();
        let (grid_delta, _grid_profit) = grid_search(upper, n, &profit_at);
        let wall_clock_micros = start.elapsed().as_micros();

        let relative_diff = if guaranteed_delta.is_zero() {
            (grid_delta - guaranteed_delta).abs()
        } else {
            (grid_delta - guaranteed_delta).abs() / guaranteed_delta.abs()
        };

        if min_converging_n.is_none() && relative_diff < ACCURACY_TOLERANCE {
            min_converging_n = Some(n);
        }

        grid_rows.push(GridRow {
            n,
            delta: grid_delta,
            wall_clock_micros,
            relative_diff,
        });
    }

    CurveBenchResult {
        name,
        guaranteed_method,
        guaranteed_delta,
        guaranteed_iterations,
        guaranteed_wall_clock_micros,
        grid_rows,
        min_converging_n,
    }
}

fn render_report(results: &[CurveBenchResult]) -> String {
    let mut out = String::new();
    writeln!(out, "# Benchmark report").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Generated by `sizing-bench`, not hand-written — regenerate with `cargo run -p sizing-bench`. \
         Per `docs/TESTING.md § Benchmark suite`: wall-clock/iteration-count comparison at grid densities \
         10 / 100 / 1,000 / 10,000, and accuracy-vs-grid-density across all 7 curve families."
    )
    .unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## Wall-clock and iteration count").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "| Curve | Method | Guaranteed iterations | Guaranteed wall-clock (µs) | Grid N | Grid wall-clock (µs) |"
    )
    .unwrap();
    writeln!(out, "|---|---|---|---|---|---|").unwrap();
    for r in results {
        for (i, row) in r.grid_rows.iter().enumerate() {
            if i == 0 {
                writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} |",
                    r.name,
                    r.guaranteed_method,
                    r.guaranteed_iterations,
                    r.guaranteed_wall_clock_micros,
                    row.n,
                    row.wall_clock_micros
                )
                .unwrap();
            } else {
                writeln!(out, "| | | | | {} | {} |", row.n, row.wall_clock_micros).unwrap();
            }
        }
    }
    writeln!(out).unwrap();

    writeln!(out, "## Accuracy vs. grid density").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "| Curve | Guaranteed Δx* | Grid N | Grid Δx | Relative diff | Within 1e-4? |"
    )
    .unwrap();
    writeln!(out, "|---|---|---|---|---|---|").unwrap();
    for r in results {
        for row in &r.grid_rows {
            writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                r.name,
                r.guaranteed_delta,
                row.n,
                row.delta,
                row.relative_diff,
                if row.relative_diff < ACCURACY_TOLERANCE {
                    "yes"
                } else {
                    "no"
                }
            )
            .unwrap();
        }
        let min_n = r
            .min_converging_n
            .map(|n| n.to_string())
            .unwrap_or_else(|| "none tested converged".to_string());
        writeln!(
            out,
            "| **{}** | | | **minimum grid N to converge within 1e-4** | | **{}** |",
            r.name, min_n
        )
        .unwrap();
    }

    out
}

fn main() {
    let results = vec![
        bench_cpmm(),
        bench_stableswap(),
        bench_pmm(),
        bench_clmm(),
        bench_balancer(),
        bench_dodo(),
        bench_cryptoswap(),
    ];
    let report = render_report(&results);

    std::fs::write("docs/benchmark-report.md", &report)
        .expect("failed to write docs/benchmark-report.md — run from the workspace root");

    println!("{report}");
}
