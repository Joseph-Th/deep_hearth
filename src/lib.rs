//! Headless deterministic simulation core for Deep Hearth.

#![forbid(unsafe_code)]
// Unit tests deliberately use unwrap/expect as assertions: a missing value or rejection
// must fail the test. Production builds retain Cargo's deny-level panic hygiene lints.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

// Foundational vocabulary. Higher-level workflows may depend on these; these stay workflow-agnostic.
pub mod capability;
pub mod core;
pub mod maintenance;
pub mod material;
pub mod spatial;

// Immutable authored definition aggregation and cross-registry validation.
pub mod content;
pub mod registry;

// Durable runtime-owner domains represented under AppState::SystemState.
pub mod combat;
pub mod energy;
pub mod equipment;
pub mod fluid;
pub mod geology;
pub mod inventory;
pub mod kobolds;
pub mod labor;
pub mod logistics;
pub mod mining;
pub mod player;
pub mod production;
pub mod structural;
pub mod surface;
pub mod survival;
pub mod world;

// Stateless/read-mostly transformation overlays that resolve work into durable owner operations.
pub mod crafting;
pub mod ore_processing;
pub mod thermal;

// Cross-owner read-only reconciliation.
pub mod matter;

// Trusted-load promotion and top-level deterministic orchestration.
pub mod persistence;
pub mod simulation;
pub mod world_generation;

// Renderer-neutral presentation definitions and deterministic assembly.
pub mod shader;
pub mod texture;

pub use content::build_registries;
pub use core::state::AppState;
pub use core::time::{SimulationTick, TickSpan};
pub use registry::Registries;
pub use simulation::{TickError, TickOutcome, advance_tick};
