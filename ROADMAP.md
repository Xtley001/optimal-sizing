# Roadmap

Everything in this file is unstarted. For completed and shipped features, see [CHANGELOG.md](./CHANGELOG.md).

## Formal SMT Verification

The current guarantee tiers rest on analytical proofs (CPMM, DLMM), stated regularity conditions (StableSwap, Velodrome), and empirical unimodality validations (PMM).
- **Scope**: An SMT-solver-backed (Z3 / Coq / Lean) formal verification harness ensuring CPMM strict concavity and Newton convergence basins hold universally across the 96-bit fixed-point `Decimal` domain.
- **Target**: Strengthening `ProvenOptimal` into a mechanically verified proof artifact.

## Hardware Acceleration and SIMD

For ultra-low latency MEV searchers operating on co-located bare metal:
- **SIMD Vectorization**: AVX-512 and ARM NEON parallelization of Golden-Section search evaluations across 8–16 simultaneous pools in a single CPU cycle.
- **FPGA Offload**: Bitstream specification for hardware-accelerated CPMM and StableSwap trade sizing in sub-50 nanosecond execution windows.

## Dynamic Invariant Ramping

Currently $A$ is supplied as a static snapshot parameter at evaluation time.
- **Scope**: Native interpolation support for Curve pools undergoing active $A$-ramping schedules over block time windows without requiring external re-fetch.
