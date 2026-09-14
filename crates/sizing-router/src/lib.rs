//! `sizing-router` — 2-hop route composition over `sizing-core`'s three
//! curves. See `docs/ARCHITECTURE.md § sizing-router` for the design
//! rationale (why this isn't just "call `optimal_size` twice") and for
//! why a composed route gets `GuaranteeTier::ComposedFrom`, never
//! `ProvenOptimal`, regardless of the legs' individual tiers.
#![warn(missing_docs)]

/// `Leg`, `Route` — the only two public types in `sizing-router`.
pub mod route;

pub use route::{Leg, Route};
