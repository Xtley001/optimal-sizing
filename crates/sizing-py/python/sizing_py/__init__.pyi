from decimal import Decimal
from typing import Optional, List, Dict, Any, Union

class SizingResult:
    optimal_delta: Decimal
    expected_profit: Decimal
    guarantee_tier: str
    guarantee_tier_detail: Dict[str, Any]
    iterations: Optional[int]

class PySizingConstraints:
    fixed_cost: Decimal
    max_size: Optional[Decimal]
    def __init__(
        self,
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> None: ...

class AllocatedPosition:
    id: str
    allocated_size: Decimal
    expected_profit: Decimal
    guarantee_tier: str

class PortfolioResult:
    total_capital_used: Decimal
    total_expected_profit: Decimal
    shadow_price: Decimal
    allocations: List[AllocatedPosition]

class PyCpmm:
    def __init__(
        self,
        x: Union[Decimal, str, int, float],
        y: Union[Decimal, str, int, float],
        fee_retention: Union[Decimal, str, int, float],
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyStableSwap:
    def __init__(
        self,
        reserves: List[Union[Decimal, str, int, float]],
        amplification: Union[Decimal, str, int, float],
        fee_retention: Union[Decimal, str, int, float],
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyPmm:
    def __init__(
        self,
        base_reserve: Union[Decimal, str, int, float],
        quote_reserve: Union[Decimal, str, int, float],
        oracle_price: Union[Decimal, str, int, float],
        k: Union[Decimal, str, int, float],
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyConcentratedLiquidity:
    def __init__(
        self,
        liquidity: Union[Decimal, str, int, float],
        sqrt_price_lower: Union[Decimal, str, int, float],
        sqrt_price_upper: Union[Decimal, str, int, float],
        sqrt_price_current: Union[Decimal, str, int, float],
        fee_retention: Union[Decimal, str, int, float],
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyBalancerWeightedPool:
    def __init__(
        self,
        reserves: List[Union[Decimal, str, int, float]],
        weights: List[Union[Decimal, str, int, float]],
        fee_retention: Union[Decimal, str, int, float],
        input_index: int = 0,
        output_index: int = 1,
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyVelodromeStable:
    def __init__(
        self,
        reserve_x: Union[Decimal, str, int, float],
        reserve_y: Union[Decimal, str, int, float],
        fee_retention: Union[Decimal, str, int, float],
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyDlmmBook:
    def __init__(
        self,
        bins: List[Dict[str, Any]],
        active_bin_index: int,
        fee_retention: Union[Decimal, str, int, float],
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyCurveCryptoSwap:
    def __init__(
        self,
        balances: List[Union[Decimal, str, int, float]],
        amplification_a: Union[Decimal, str, int, float],
        gamma: Union[Decimal, str, int, float],
        fee_retention: Union[Decimal, str, int, float],
        input_index: int = 0,
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

class PyDodoPmm:
    def __init__(
        self,
        base_target: Union[Decimal, str, int, float],
        quote_target: Union[Decimal, str, int, float],
        base_reserve: Union[Decimal, str, int, float],
        quote_reserve: Union[Decimal, str, int, float],
        oracle_price: Union[Decimal, str, int, float],
        k: Union[Decimal, str, int, float],
        fee_retention: Union[Decimal, str, int, float],
        is_sell_base: bool = True,
    ) -> None: ...
    def quote(self, delta_in: Union[Decimal, str, int, float]) -> Decimal: ...
    def optimal_size(
        self,
        reference_price: Union[Decimal, str, int, float],
        fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
        max_size: Optional[Union[Decimal, str, int, float]] = None,
    ) -> SizingResult: ...

def cpmm_optimal_size(
    x: Union[Decimal, str, int, float],
    y: Union[Decimal, str, int, float],
    fee_retention: Union[Decimal, str, int, float],
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def stableswap_optimal_size(
    reserves: List[Union[Decimal, str, int, float]],
    amplification: Union[Decimal, str, int, float],
    fee_retention: Union[Decimal, str, int, float],
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def pmm_optimal_size(
    base_reserve: Union[Decimal, str, int, float],
    quote_reserve: Union[Decimal, str, int, float],
    oracle_price: Union[Decimal, str, int, float],
    k: Union[Decimal, str, int, float],
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def clmm_optimal_size(
    liquidity: Union[Decimal, str, int, float],
    sqrt_price_lower: Union[Decimal, str, int, float],
    sqrt_price_upper: Union[Decimal, str, int, float],
    sqrt_price_current: Union[Decimal, str, int, float],
    fee_retention: Union[Decimal, str, int, float],
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def balancer_optimal_size(
    reserves: List[Union[Decimal, str, int, float]],
    weights: List[Union[Decimal, str, int, float]],
    fee_retention: Union[Decimal, str, int, float],
    input_index: int,
    output_index: int,
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def velodrome_optimal_size(
    reserve_x: Union[Decimal, str, int, float],
    reserve_y: Union[Decimal, str, int, float],
    fee_retention: Union[Decimal, str, int, float],
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def dlmm_optimal_size(
    bins: List[Dict[str, Any]],
    active_bin_index: int,
    fee_retention: Union[Decimal, str, int, float],
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def cryptoswap_optimal_size(
    balances: List[Union[Decimal, str, int, float]],
    amplification_a: Union[Decimal, str, int, float],
    gamma: Union[Decimal, str, int, float],
    fee_retention: Union[Decimal, str, int, float],
    input_index: int,
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def dodo_optimal_size(
    base_target: Union[Decimal, str, int, float],
    quote_target: Union[Decimal, str, int, float],
    base_reserve: Union[Decimal, str, int, float],
    quote_reserve: Union[Decimal, str, int, float],
    oracle_price: Union[Decimal, str, int, float],
    k: Union[Decimal, str, int, float],
    fee_retention: Union[Decimal, str, int, float],
    is_sell_base: bool,
    reference_price: Union[Decimal, str, int, float],
    fixed_cost: Optional[Union[Decimal, str, int, float]] = None,
    max_size: Optional[Union[Decimal, str, int, float]] = None,
) -> SizingResult: ...

def portfolio_optimize(
    total_capital_budget: Union[Decimal, str, int, float],
    opportunities: List[Dict[str, Any]],
) -> PortfolioResult: ...
