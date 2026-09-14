//! `sizing-py` — thin PyO3 wrapper around `sizing-core`'s curve families.
//! Boundary-only: every numeric input accepts anything Python can `str()`,
//! and every numeric output is returned as a real Python `decimal.Decimal`.

use std::str::FromStr;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use rust_decimal::Decimal;

use sizing_core::curves::{BalancerWeightedPool, ConcentratedLiquidity, Cpmm, Pmm, StableSwap};
use sizing_core::traits::SizingAlgorithm;
use sizing_core::types::{GuaranteeTier, SizingConstraints, SizingResult as CoreSizingResult};

fn parse_decimal(obj: &Bound<'_, PyAny>, field: &str) -> PyResult<Decimal> {
    let s: String = obj.str()?.extract()?;
    Decimal::from_str(&s).map_err(|e| PyValueError::new_err(format!("invalid {field} {s:?}: {e}")))
}

fn parse_optional_decimal(obj: Option<&Bound<'_, PyAny>>, field: &str) -> PyResult<Option<Decimal>> {
    obj.map(|o| parse_decimal(o, field)).transpose()
}

fn to_py_decimal(py: Python<'_>, value: Decimal) -> PyResult<PyObject> {
    let decimal_module = py.import_bound("decimal")?;
    let decimal_class = decimal_module.getattr("Decimal")?;
    let obj = decimal_class.call1((value.to_string(),))?;
    Ok(obj.unbind())
}

fn tier_tag(tier: &GuaranteeTier) -> &'static str {
    match tier {
        GuaranteeTier::ProvenOptimal => "ProvenOptimal",
        GuaranteeTier::NumericallyGuaranteed { .. } => "NumericallyGuaranteed",
        GuaranteeTier::EmpiricallyValidated { .. } => "EmpiricallyValidated",
        GuaranteeTier::ComposedFrom(_) => "ComposedFrom",
    }
}

/// Python-facing sizing result.
#[pyclass]
pub struct SizingResult {
    #[pyo3(get)]
    optimal_delta: PyObject,
    #[pyo3(get)]
    expected_profit: PyObject,
    #[pyo3(get)]
    guarantee_tier: String,
    #[pyo3(get)]
    guarantee_tier_detail: PyObject,
    #[pyo3(get)]
    iterations: Option<u32>,
}

fn tier_detail(py: Python<'_>, tier: &GuaranteeTier) -> PyResult<PyObject> {
    use pyo3::types::PyDict;
    let dict = PyDict::new_bound(py);
    match tier {
        GuaranteeTier::ProvenOptimal => {}
        GuaranteeTier::NumericallyGuaranteed { convergence_conditions_met } => {
            dict.set_item("convergence_conditions_met", *convergence_conditions_met)?;
        }
        GuaranteeTier::EmpiricallyValidated { unimodality_confirmed } => {
            dict.set_item("unimodality_confirmed", *unimodality_confirmed)?;
        }
        GuaranteeTier::ComposedFrom(legs) => {
            let tags: Vec<&'static str> = legs.iter().map(tier_tag).collect();
            dict.set_item("legs", tags)?;
        }
    }
    Ok(dict.into())
}

fn to_py_result(py: Python<'_>, r: CoreSizingResult) -> PyResult<SizingResult> {
    Ok(SizingResult {
        optimal_delta: to_py_decimal(py, r.optimal_delta)?,
        expected_profit: to_py_decimal(py, r.expected_profit)?,
        guarantee_tier: tier_tag(&r.guarantee_tier).to_string(),
        guarantee_tier_detail: tier_detail(py, &r.guarantee_tier)?,
        iterations: r.iterations,
    })
}

fn map_sizing_error(e: sizing_core::error::SizingError) -> PyErr {
    PyValueError::new_err(format!("{e:?}"))
}

/// Sizes a constant-product (`x*y=k`) pool.
#[pyfunction]
#[pyo3(signature = (x, y, fee_retention, reference_price, fixed_cost, max_size=None))]
fn cpmm_optimal_size(
    py: Python<'_>,
    x: &Bound<'_, PyAny>,
    y: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: &Bound<'_, PyAny>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let x = parse_decimal(x, "x")?;
    let y = parse_decimal(y, "y")?;
    let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
    let reference_price = parse_decimal(reference_price, "reference_price")?;
    let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
    let max_size = parse_optional_decimal(max_size, "max_size")?;

    let pool = Cpmm::new(x, y, fee_retention).map_err(map_sizing_error)?;
    let result = pool
        .optimal_size(reference_price, SizingConstraints { fixed_cost, max_size })
        .map_err(map_sizing_error)?;
    to_py_result(py, result)
}

/// Sizes a StableSwap-invariant pool.
#[pyfunction]
#[pyo3(signature = (reserves, amplification, fee_retention, reference_price, fixed_cost, max_size=None))]
fn stableswap_optimal_size(
    py: Python<'_>,
    reserves: Vec<Bound<'_, PyAny>>,
    amplification: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: &Bound<'_, PyAny>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let reserves: PyResult<Vec<Decimal>> =
        reserves.iter().map(|r| parse_decimal(r, "reserves[i]")).collect();
    let reserves = reserves?;
    let amplification = parse_decimal(amplification, "amplification")?;
    let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
    let reference_price = parse_decimal(reference_price, "reference_price")?;
    let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
    let max_size = parse_optional_decimal(max_size, "max_size")?;

    let pool = StableSwap::new(reserves, amplification, fee_retention).map_err(map_sizing_error)?;
    let result = pool
        .optimal_size(reference_price, SizingConstraints { fixed_cost, max_size })
        .map_err(map_sizing_error)?;
    to_py_result(py, result)
}

/// Sizes a WooFi-style PMM pool.
#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (base_reserve, quote_reserve, oracle_price, k, reference_price, fixed_cost, max_size=None))]
fn pmm_optimal_size(
    py: Python<'_>,
    base_reserve: &Bound<'_, PyAny>,
    quote_reserve: &Bound<'_, PyAny>,
    oracle_price: &Bound<'_, PyAny>,
    k: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: &Bound<'_, PyAny>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let base_reserve = parse_decimal(base_reserve, "base_reserve")?;
    let quote_reserve = parse_decimal(quote_reserve, "quote_reserve")?;
    let oracle_price = parse_decimal(oracle_price, "oracle_price")?;
    let k = parse_decimal(k, "k")?;
    let reference_price = parse_decimal(reference_price, "reference_price")?;
    let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
    let max_size = parse_optional_decimal(max_size, "max_size")?;

    let pool = Pmm::new(base_reserve, quote_reserve, oracle_price, k).map_err(map_sizing_error)?;
    let result = pool
        .optimal_size(reference_price, SizingConstraints { fixed_cost, max_size })
        .map_err(map_sizing_error)?;
    to_py_result(py, result)
}

/// Sizes a concentrated-liquidity position.
#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (liquidity, sqrt_price_lower, sqrt_price_upper, sqrt_price_current, fee_retention, reference_price, fixed_cost, max_size=None))]
fn clmm_optimal_size(
    py: Python<'_>,
    liquidity: &Bound<'_, PyAny>,
    sqrt_price_lower: &Bound<'_, PyAny>,
    sqrt_price_upper: &Bound<'_, PyAny>,
    sqrt_price_current: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: &Bound<'_, PyAny>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let liquidity = parse_decimal(liquidity, "liquidity")?;
    let sqrt_price_lower = parse_decimal(sqrt_price_lower, "sqrt_price_lower")?;
    let sqrt_price_upper = parse_decimal(sqrt_price_upper, "sqrt_price_upper")?;
    let sqrt_price_current = parse_decimal(sqrt_price_current, "sqrt_price_current")?;
    let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
    let reference_price = parse_decimal(reference_price, "reference_price")?;
    let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
    let max_size = parse_optional_decimal(max_size, "max_size")?;

    let pool = ConcentratedLiquidity::new(
        liquidity,
        sqrt_price_lower,
        sqrt_price_upper,
        sqrt_price_current,
        fee_retention,
    )
    .map_err(map_sizing_error)?;
    let result = pool
        .optimal_size(reference_price, SizingConstraints { fixed_cost, max_size })
        .map_err(map_sizing_error)?;
    to_py_result(py, result)
}

/// Sizes a Balancer weighted pool.
#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (reserves, weights, fee_retention, input_index, output_index, reference_price, fixed_cost, max_size=None))]
fn balancer_optimal_size(
    py: Python<'_>,
    reserves: Vec<Bound<'_, PyAny>>,
    weights: Vec<Bound<'_, PyAny>>,
    fee_retention: &Bound<'_, PyAny>,
    input_index: usize,
    output_index: usize,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: &Bound<'_, PyAny>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let reserves: PyResult<Vec<Decimal>> =
        reserves.iter().map(|r| parse_decimal(r, "reserves[i]")).collect();
    let reserves = reserves?;
    let weights: PyResult<Vec<Decimal>> =
        weights.iter().map(|w| parse_decimal(w, "weights[i]")).collect();
    let weights = weights?;
    let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
    let reference_price = parse_decimal(reference_price, "reference_price")?;
    let fixed_cost = parse_decimal(fixed_cost, "fixed_cost")?;
    let max_size = parse_optional_decimal(max_size, "max_size")?;

    let pool = BalancerWeightedPool::new(reserves, weights, fee_retention, input_index, output_index)
        .map_err(map_sizing_error)?;
    let result = pool
        .optimal_size(reference_price, SizingConstraints { fixed_cost, max_size })
        .map_err(map_sizing_error)?;
    to_py_result(py, result)
}

/// The `sizing_py` Python module.
#[pymodule]
fn sizing_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SizingResult>()?;
    m.add_function(wrap_pyfunction!(cpmm_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(stableswap_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(pmm_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(clmm_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(balancer_optimal_size, m)?)?;
    Ok(())
}
