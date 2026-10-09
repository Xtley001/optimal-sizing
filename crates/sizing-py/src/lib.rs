//! `sizing-py` — Comprehensive PyO3 Python bindings for `sizing-core` and `sizing-portfolio`.
//! Boundary-only: numeric inputs accept anything Python can `str()` or `Decimal`,
//! and numeric outputs are returned as native Python `decimal.Decimal`.

use std::str::FromStr;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};
use pyo3::Py;
use rust_decimal::Decimal;

type PyObject = Py<PyAny>;

use sizing_core::curves::{
    BalancerWeightedPool, ConcentratedLiquidity, Cpmm, CurveCryptoSwap, DlmmBin, DodoPmm,
    LiquidityBook, Pmm, StableSwap, VelodromeStable,
};
use sizing_core::traits::{PricingCurve, SizingAlgorithm};
use sizing_core::types::{GuaranteeTier, SizingConstraints, SizingResult as CoreSizingResult};
use sizing_portfolio::{PortfolioOpportunity, PortfolioResult as CorePortfolioResult, PortfolioSizer};

fn parse_decimal(obj: &Bound<'_, PyAny>, field: &str) -> PyResult<Decimal> {
    let s: String = obj.str()?.extract()?;
    Decimal::from_str(&s).map_err(|e| PyValueError::new_err(format!("invalid {field} {s:?}: {e}")))
}

fn parse_optional_decimal(obj: Option<&Bound<'_, PyAny>>, field: &str) -> PyResult<Option<Decimal>> {
    obj.map(|o| parse_decimal(o, field)).transpose()
}

fn to_py_decimal(py: Python<'_>, value: Decimal) -> PyResult<PyObject> {
    let decimal_module = py.import("decimal")?;
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

fn tier_detail(py: Python<'_>, tier: &GuaranteeTier) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
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
    Ok(dict.into_any().unbind())
}

fn map_sizing_error(e: sizing_core::error::SizingError) -> PyErr {
    PyValueError::new_err(format!("{e:?}"))
}

// ---------------------------------------------------------------------------
// Result Structures
// ---------------------------------------------------------------------------

/// Result of an individual AMM sizing calculation.
#[pyclass]
pub struct SizingResult {
    #[pyo3(get)]
    pub optimal_delta: PyObject,
    #[pyo3(get)]
    pub expected_profit: PyObject,
    #[pyo3(get)]
    pub guarantee_tier: String,
    #[pyo3(get)]
    pub guarantee_tier_detail: PyObject,
    #[pyo3(get)]
    pub iterations: Option<u32>,
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

/// Sizing constraints wrapper.
#[pyclass]
pub struct PySizingConstraints {
    pub fixed_cost: Decimal,
    pub max_size: Option<Decimal>,
}

#[pymethods]
impl PySizingConstraints {
    #[new]
    #[pyo3(signature = (fixed_cost=None, max_size=None))]
    fn new(fixed_cost: Option<&Bound<'_, PyAny>>, max_size: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        Ok(Self { fixed_cost, max_size })
    }
}

/// Position allocation in a solved portfolio.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct AllocatedPosition {
    #[pyo3(get)]
    pub id: String,
    pub allocated_size: Decimal,
    pub expected_profit: Decimal,
    #[pyo3(get)]
    pub guarantee_tier: String,
}

#[pymethods]
impl AllocatedPosition {
    #[getter]
    pub fn allocated_size(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_decimal(py, self.allocated_size)
    }

    #[getter]
    pub fn expected_profit(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_decimal(py, self.expected_profit)
    }
}

/// Global portfolio optimization result.
#[pyclass]
pub struct PortfolioResult {
    pub total_capital_used: Decimal,
    pub total_expected_profit: Decimal,
    pub shadow_price: Decimal,
    pub allocations: Vec<AllocatedPosition>,
}

#[pymethods]
impl PortfolioResult {
    #[getter]
    pub fn total_capital_used(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_decimal(py, self.total_capital_used)
    }

    #[getter]
    pub fn total_expected_profit(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_decimal(py, self.total_expected_profit)
    }

    #[getter]
    pub fn shadow_price(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_decimal(py, self.shadow_price)
    }

    #[getter]
    pub fn allocations(&self) -> Vec<AllocatedPosition> {
        self.allocations.clone()
    }
}

// ---------------------------------------------------------------------------
// Object-Oriented Curve Wrappers
// ---------------------------------------------------------------------------

#[pyclass]
pub struct PyCpmm {
    inner: Cpmm,
}

#[pymethods]
impl PyCpmm {
    #[new]
    #[pyo3(signature = (x, y, fee_retention))]
    pub fn new(x: &Bound<'_, PyAny>, y: &Bound<'_, PyAny>, fee_retention: &Bound<'_, PyAny>) -> PyResult<Self> {
        let x = parse_decimal(x, "x")?;
        let y = parse_decimal(y, "y")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = Cpmm::new(x, y, fee_retention).map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyStableSwap {
    inner: StableSwap,
}

#[pymethods]
impl PyStableSwap {
    #[new]
    #[pyo3(signature = (reserves, amplification, fee_retention))]
    pub fn new(
        reserves: Vec<Bound<'_, PyAny>>,
        amplification: &Bound<'_, PyAny>,
        fee_retention: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let reserves: PyResult<Vec<Decimal>> = reserves.iter().map(|r| parse_decimal(r, "reserves[i]")).collect();
        let reserves = reserves?;
        let amplification = parse_decimal(amplification, "amplification")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = StableSwap::new(reserves, amplification, fee_retention).map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyPmm {
    inner: Pmm,
}

#[pymethods]
impl PyPmm {
    #[new]
    #[pyo3(signature = (base_reserve, quote_reserve, oracle_price, k))]
    pub fn new(
        base_reserve: &Bound<'_, PyAny>,
        quote_reserve: &Bound<'_, PyAny>,
        oracle_price: &Bound<'_, PyAny>,
        k: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let base_reserve = parse_decimal(base_reserve, "base_reserve")?;
        let quote_reserve = parse_decimal(quote_reserve, "quote_reserve")?;
        let oracle_price = parse_decimal(oracle_price, "oracle_price")?;
        let k = parse_decimal(k, "k")?;
        let inner = Pmm::new(base_reserve, quote_reserve, oracle_price, k).map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyConcentratedLiquidity {
    inner: ConcentratedLiquidity,
}

#[pymethods]
impl PyConcentratedLiquidity {
    #[new]
    #[pyo3(signature = (liquidity, sqrt_price_lower, sqrt_price_upper, sqrt_price_current, fee_retention))]
    pub fn new(
        liquidity: &Bound<'_, PyAny>,
        sqrt_price_lower: &Bound<'_, PyAny>,
        sqrt_price_upper: &Bound<'_, PyAny>,
        sqrt_price_current: &Bound<'_, PyAny>,
        fee_retention: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let liquidity = parse_decimal(liquidity, "liquidity")?;
        let sqrt_price_lower = parse_decimal(sqrt_price_lower, "sqrt_price_lower")?;
        let sqrt_price_upper = parse_decimal(sqrt_price_upper, "sqrt_price_upper")?;
        let sqrt_price_current = parse_decimal(sqrt_price_current, "sqrt_price_current")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = ConcentratedLiquidity::new(
            liquidity,
            sqrt_price_lower,
            sqrt_price_upper,
            sqrt_price_current,
            fee_retention,
        )
        .map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyBalancerWeightedPool {
    inner: BalancerWeightedPool,
}

#[pymethods]
impl PyBalancerWeightedPool {
    #[new]
    #[pyo3(signature = (reserves, weights, fee_retention, input_index=0, output_index=1))]
    pub fn new(
        reserves: Vec<Bound<'_, PyAny>>,
        weights: Vec<Bound<'_, PyAny>>,
        fee_retention: &Bound<'_, PyAny>,
        input_index: usize,
        output_index: usize,
    ) -> PyResult<Self> {
        let reserves: PyResult<Vec<Decimal>> = reserves.iter().map(|r| parse_decimal(r, "reserves[i]")).collect();
        let reserves = reserves?;
        let weights: PyResult<Vec<Decimal>> = weights.iter().map(|w| parse_decimal(w, "weights[i]")).collect();
        let weights = weights?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = BalancerWeightedPool::new(reserves, weights, fee_retention, input_index, output_index)
            .map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyVelodromeStable {
    inner: VelodromeStable,
}

#[pymethods]
impl PyVelodromeStable {
    #[new]
    #[pyo3(signature = (reserve_x, reserve_y, fee_retention))]
    pub fn new(
        reserve_x: &Bound<'_, PyAny>,
        reserve_y: &Bound<'_, PyAny>,
        fee_retention: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let reserve_x = parse_decimal(reserve_x, "reserve_x")?;
        let reserve_y = parse_decimal(reserve_y, "reserve_y")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = VelodromeStable::new(reserve_x, reserve_y, fee_retention).map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyDlmmBook {
    inner: LiquidityBook,
}

#[pymethods]
impl PyDlmmBook {
    #[new]
    #[pyo3(signature = (bins, active_bin_index, fee_retention))]
    pub fn new(
        bins: Vec<Bound<'_, PyAny>>,
        active_bin_index: usize,
        fee_retention: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let mut rust_bins = Vec::with_capacity(bins.len());
        for b in &bins {
            let bin_id: i32 = b.get_item("bin_id")?.extract()?;
            let price_obj = b.get_item("price")?;
            let rx_obj = b.get_item("reserve_x")?;
            let ry_obj = b.get_item("reserve_y")?;
            rust_bins.push(DlmmBin {
                bin_id,
                price: parse_decimal(&price_obj, "bin.price")?,
                reserve_x: parse_decimal(&rx_obj, "bin.reserve_x")?,
                reserve_y: parse_decimal(&ry_obj, "bin.reserve_y")?,
            });
        }
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = LiquidityBook::new(rust_bins, active_bin_index, fee_retention).map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyCurveCryptoSwap {
    inner: CurveCryptoSwap,
}

#[pymethods]
impl PyCurveCryptoSwap {
    #[new]
    #[pyo3(signature = (balances, amplification_a, gamma, fee_retention, input_index=0))]
    pub fn new(
        balances: Vec<Bound<'_, PyAny>>,
        amplification_a: &Bound<'_, PyAny>,
        gamma: &Bound<'_, PyAny>,
        fee_retention: &Bound<'_, PyAny>,
        input_index: usize,
    ) -> PyResult<Self> {
        if balances.len() != 2 {
            return Err(PyValueError::new_err("balances must contain exactly 2 elements"));
        }
        let b0 = parse_decimal(&balances[0], "balances[0]")?;
        let b1 = parse_decimal(&balances[1], "balances[1]")?;
        let amplification_a = parse_decimal(amplification_a, "amplification_a")?;
        let gamma = parse_decimal(gamma, "gamma")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = CurveCryptoSwap::new([b0, b1], amplification_a, gamma, fee_retention, input_index)
            .map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

#[pyclass]
pub struct PyDodoPmm {
    inner: DodoPmm,
}

#[pymethods]
impl PyDodoPmm {
    #[allow(clippy::too_many_arguments)]
    #[new]
    #[pyo3(signature = (base_target, quote_target, base_reserve, quote_reserve, oracle_price, k, fee_retention, is_sell_base=true))]
    pub fn new(
        base_target: &Bound<'_, PyAny>,
        quote_target: &Bound<'_, PyAny>,
        base_reserve: &Bound<'_, PyAny>,
        quote_reserve: &Bound<'_, PyAny>,
        oracle_price: &Bound<'_, PyAny>,
        k: &Bound<'_, PyAny>,
        fee_retention: &Bound<'_, PyAny>,
        is_sell_base: bool,
    ) -> PyResult<Self> {
        let base_target = parse_decimal(base_target, "base_target")?;
        let quote_target = parse_decimal(quote_target, "quote_target")?;
        let base_reserve = parse_decimal(base_reserve, "base_reserve")?;
        let quote_reserve = parse_decimal(quote_reserve, "quote_reserve")?;
        let oracle_price = parse_decimal(oracle_price, "oracle_price")?;
        let k = parse_decimal(k, "k")?;
        let fee_retention = parse_decimal(fee_retention, "fee_retention")?;
        let inner = DodoPmm::new(
            base_target,
            quote_target,
            base_reserve,
            quote_reserve,
            oracle_price,
            k,
            fee_retention,
            is_sell_base,
        )
        .map_err(map_sizing_error)?;
        Ok(Self { inner })
    }

    pub fn quote(&self, py: Python<'_>, delta_in: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let delta_in = parse_decimal(delta_in, "delta_in")?;
        let out = self.inner.quote(delta_in).map_err(map_sizing_error)?;
        to_py_decimal(py, out)
    }

    #[pyo3(signature = (reference_price, fixed_cost=None, max_size=None))]
    pub fn optimal_size(
        &self,
        py: Python<'_>,
        reference_price: &Bound<'_, PyAny>,
        fixed_cost: Option<&Bound<'_, PyAny>>,
        max_size: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<SizingResult> {
        let reference_price = parse_decimal(reference_price, "reference_price")?;
        let fixed_cost = parse_optional_decimal(fixed_cost, "fixed_cost")?.unwrap_or(Decimal::ZERO);
        let max_size = parse_optional_decimal(max_size, "max_size")?;
        let res = self.inner.optimal_size(reference_price, SizingConstraints { fixed_cost, max_size }).map_err(map_sizing_error)?;
        to_py_result(py, res)
    }
}

// ---------------------------------------------------------------------------
// Standalone Functions
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (x, y, fee_retention, reference_price, fixed_cost=None, max_size=None))]
fn cpmm_optimal_size(
    py: Python<'_>,
    x: &Bound<'_, PyAny>,
    y: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyCpmm::new(x, y, fee_retention)?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[pyfunction]
#[pyo3(signature = (reserves, amplification, fee_retention, reference_price, fixed_cost=None, max_size=None))]
fn stableswap_optimal_size(
    py: Python<'_>,
    reserves: Vec<Bound<'_, PyAny>>,
    amplification: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyStableSwap::new(reserves, amplification, fee_retention)?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[pyfunction]
#[pyo3(signature = (base_reserve, quote_reserve, oracle_price, k, reference_price, fixed_cost=None, max_size=None))]
fn pmm_optimal_size(
    py: Python<'_>,
    base_reserve: &Bound<'_, PyAny>,
    quote_reserve: &Bound<'_, PyAny>,
    oracle_price: &Bound<'_, PyAny>,
    k: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyPmm::new(base_reserve, quote_reserve, oracle_price, k)?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (liquidity, sqrt_price_lower, sqrt_price_upper, sqrt_price_current, fee_retention, reference_price, fixed_cost=None, max_size=None))]
fn clmm_optimal_size(
    py: Python<'_>,
    liquidity: &Bound<'_, PyAny>,
    sqrt_price_lower: &Bound<'_, PyAny>,
    sqrt_price_upper: &Bound<'_, PyAny>,
    sqrt_price_current: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyConcentratedLiquidity::new(
        liquidity,
        sqrt_price_lower,
        sqrt_price_upper,
        sqrt_price_current,
        fee_retention,
    )?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (reserves, weights, fee_retention, input_index, output_index, reference_price, fixed_cost=None, max_size=None))]
fn balancer_optimal_size(
    py: Python<'_>,
    reserves: Vec<Bound<'_, PyAny>>,
    weights: Vec<Bound<'_, PyAny>>,
    fee_retention: &Bound<'_, PyAny>,
    input_index: usize,
    output_index: usize,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyBalancerWeightedPool::new(reserves, weights, fee_retention, input_index, output_index)?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[pyfunction]
#[pyo3(signature = (reserve_x, reserve_y, fee_retention, reference_price, fixed_cost=None, max_size=None))]
fn velodrome_optimal_size(
    py: Python<'_>,
    reserve_x: &Bound<'_, PyAny>,
    reserve_y: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyVelodromeStable::new(reserve_x, reserve_y, fee_retention)?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[pyfunction]
#[pyo3(signature = (bins, active_bin_index, fee_retention, reference_price, fixed_cost=None, max_size=None))]
fn dlmm_optimal_size(
    py: Python<'_>,
    bins: Vec<Bound<'_, PyAny>>,
    active_bin_index: usize,
    fee_retention: &Bound<'_, PyAny>,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyDlmmBook::new(bins, active_bin_index, fee_retention)?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (balances, amplification_a, gamma, fee_retention, input_index, reference_price, fixed_cost=None, max_size=None))]
fn cryptoswap_optimal_size(
    py: Python<'_>,
    balances: Vec<Bound<'_, PyAny>>,
    amplification_a: &Bound<'_, PyAny>,
    gamma: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    input_index: usize,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyCurveCryptoSwap::new(balances, amplification_a, gamma, fee_retention, input_index)?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (base_target, quote_target, base_reserve, quote_reserve, oracle_price, k, fee_retention, is_sell_base, reference_price, fixed_cost=None, max_size=None))]
fn dodo_optimal_size(
    py: Python<'_>,
    base_target: &Bound<'_, PyAny>,
    quote_target: &Bound<'_, PyAny>,
    base_reserve: &Bound<'_, PyAny>,
    quote_reserve: &Bound<'_, PyAny>,
    oracle_price: &Bound<'_, PyAny>,
    k: &Bound<'_, PyAny>,
    fee_retention: &Bound<'_, PyAny>,
    is_sell_base: bool,
    reference_price: &Bound<'_, PyAny>,
    fixed_cost: Option<&Bound<'_, PyAny>>,
    max_size: Option<&Bound<'_, PyAny>>,
) -> PyResult<SizingResult> {
    let pool = PyDodoPmm::new(
        base_target,
        quote_target,
        base_reserve,
        quote_reserve,
        oracle_price,
        k,
        fee_retention,
        is_sell_base,
    )?;
    pool.optimal_size(py, reference_price, fixed_cost, max_size)
}

// ---------------------------------------------------------------------------
// Multi-Pool Portfolio Capital Allocation
// ---------------------------------------------------------------------------

fn extract_pool_algo(obj: &Bound<'_, PyAny>) -> PyResult<Box<dyn SizingAlgorithm + Send + Sync>> {
    if let Ok(cpmm) = obj.extract::<PyRef<PyCpmm>>() {
        return Ok(Box::new(cpmm.inner.clone()));
    }
    if let Ok(ss) = obj.extract::<PyRef<PyStableSwap>>() {
        return Ok(Box::new(ss.inner.clone()));
    }
    if let Ok(pmm) = obj.extract::<PyRef<PyPmm>>() {
        return Ok(Box::new(pmm.inner.clone()));
    }
    if let Ok(clmm) = obj.extract::<PyRef<PyConcentratedLiquidity>>() {
        return Ok(Box::new(clmm.inner.clone()));
    }
    if let Ok(bal) = obj.extract::<PyRef<PyBalancerWeightedPool>>() {
        return Ok(Box::new(bal.inner.clone()));
    }
    if let Ok(velo) = obj.extract::<PyRef<PyVelodromeStable>>() {
        return Ok(Box::new(velo.inner.clone()));
    }
    if let Ok(dlmm) = obj.extract::<PyRef<PyDlmmBook>>() {
        return Ok(Box::new(dlmm.inner.clone()));
    }
    if let Ok(crypto) = obj.extract::<PyRef<PyCurveCryptoSwap>>() {
        return Ok(Box::new(crypto.inner.clone()));
    }
    if let Ok(dodo) = obj.extract::<PyRef<PyDodoPmm>>() {
        return Ok(Box::new(dodo.inner.clone()));
    }
    Err(PyValueError::new_err(
        "opportunity 'pool' must be a supported AMM curve instance (PyCpmm, PyStableSwap, PyVelodromeStable, PyDlmmBook, etc.)",
    ))
}

#[pyfunction]
#[pyo3(signature = (total_capital_budget, opportunities))]
pub fn portfolio_optimize(
    _py: Python<'_>,
    total_capital_budget: &Bound<'_, PyAny>,
    opportunities: &Bound<'_, PyList>,
) -> PyResult<PortfolioResult> {
    let budget = parse_decimal(total_capital_budget, "total_capital_budget")?;
    let sizer = PortfolioSizer::new(budget).map_err(map_sizing_error)?;

    let mut rust_opps = Vec::with_capacity(opportunities.len());
    for item in opportunities.iter() {
        let id: String = item.get_item("id")?.extract()?;
        let pool_obj = item.get_item("pool")?;
        let pool = extract_pool_algo(&pool_obj)?;
        let ref_price_obj = item.get_item("reference_price")?;
        let reference_price = parse_decimal(&ref_price_obj, "reference_price")?;

        let fixed_cost = match item.get_item("fixed_cost") {
            Ok(fc) if !fc.is_none() => parse_decimal(&fc, "fixed_cost")?,
            _ => Decimal::ZERO,
        };

        let max_size = match item.get_item("max_size") {
            Ok(ms) if !ms.is_none() => Some(parse_decimal(&ms, "max_size")?),
            _ => None,
        };

        rust_opps.push(PortfolioOpportunity {
            id,
            pool,
            reference_price,
            constraints: SizingConstraints { fixed_cost, max_size },
        });
    }

    let core_res: CorePortfolioResult = sizer.optimize(&rust_opps).map_err(map_sizing_error)?;

    let mut py_allocs = Vec::with_capacity(core_res.allocations.len());
    for alloc in core_res.allocations {
        py_allocs.push(AllocatedPosition {
            id: alloc.id,
            allocated_size: alloc.allocated_size,
            expected_profit: alloc.expected_profit,
            guarantee_tier: tier_tag(&alloc.guarantee_tier).to_string(),
        });
    }

    Ok(PortfolioResult {
        total_capital_used: core_res.total_capital_used,
        total_expected_profit: core_res.total_expected_profit,
        shadow_price: core_res.shadow_price,
        allocations: py_allocs,
    })
}

// ---------------------------------------------------------------------------
// Python Module Definition
// ---------------------------------------------------------------------------

#[pymodule]
fn sizing_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Classes
    m.add_class::<SizingResult>()?;
    m.add_class::<PySizingConstraints>()?;
    m.add_class::<AllocatedPosition>()?;
    m.add_class::<PortfolioResult>()?;
    m.add_class::<PyCpmm>()?;
    m.add_class::<PyStableSwap>()?;
    m.add_class::<PyPmm>()?;
    m.add_class::<PyConcentratedLiquidity>()?;
    m.add_class::<PyBalancerWeightedPool>()?;
    m.add_class::<PyVelodromeStable>()?;
    m.add_class::<PyDlmmBook>()?;
    m.add_class::<PyCurveCryptoSwap>()?;
    m.add_class::<PyDodoPmm>()?;

    // Standalone functions
    m.add_function(wrap_pyfunction!(cpmm_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(stableswap_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(pmm_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(clmm_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(balancer_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(velodrome_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(dlmm_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(cryptoswap_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(dodo_optimal_size, m)?)?;
    m.add_function(wrap_pyfunction!(portfolio_optimize, m)?)?;

    Ok(())
}
