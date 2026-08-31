//! Headless deterministic simulation core for Deep Hearth.

#![forbid(unsafe_code)]

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
pub mod energy;
pub mod equipment;
pub mod fluid;
pub mod geology;
pub mod inventory;
pub mod labor;
pub mod mining;
pub mod production;
pub mod structural;
pub mod survival;

// Stateless/read-mostly transformation overlays that resolve work into durable owner operations.
pub mod crafting;
pub mod ore_processing;
pub mod thermal;

// Cross-owner read-only reconciliation.
pub mod matter;

// Trusted-load promotion and top-level deterministic orchestration.
pub mod persistence;
pub mod simulation;

// Renderer-neutral presentation definitions and deterministic assembly.
pub mod shader;
pub mod texture;

pub use content::build_registries;
pub use core::state::AppState;
pub use core::time::{SimulationTick, TickSpan, WorldSeed};
pub use registry::Registries;
pub use simulation::{TickError, TickOutcome, advance_tick};
