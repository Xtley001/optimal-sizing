export interface WasmSizingResult {
  readonly ok: boolean;
  readonly optimal_delta: string;
  readonly expected_profit: string;
  readonly guarantee_tier: string;
  readonly iterations: number;
  readonly error: string;
}

export function cpmm_optimal_size(
  x: string,
  y: string,
  fee_retention: string,
  reference_price: string,
  fixed_cost: string,
  max_size?: string
): WasmSizingResult;

export function stableswap_optimal_size(
  reserves: string[],
  amplification: string,
  fee_retention: string,
  reference_price: string,
  fixed_cost: string,
  max_size?: string
): WasmSizingResult;

export function pmm_optimal_size(
  base_reserve: string,
  quote_reserve: string,
  oracle_price: string,
  k: string,
  reference_price: string,
  fixed_cost: string,
  max_size?: string
): WasmSizingResult;

export function clmm_optimal_size(
  liquidity: string,
  sqrt_price_lower: string,
  sqrt_price_upper: string,
  sqrt_price_current: string,
  fee_retention: string,
  reference_price: string,
  fixed_cost: string,
  max_size?: string
): WasmSizingResult;

export function balancer_optimal_size(
  reserves: string[],
  weights: string[],
  fee_retention: string,
  input_index: number,
  output_index: number,
  reference_price: string,
  fixed_cost: string,
  max_size?: string
): WasmSizingResult;
