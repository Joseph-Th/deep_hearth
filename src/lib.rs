//! Headless deterministic simulation core for Deep Hearth.

#![forbid(unsafe_code)]
#![cfg_attr(
    all(
        test,
        feature = "test-unit-sharded",
        not(all(
            feature = "test-unit-foundation",
            feature = "test-unit-resources",
            feature = "test-unit-player",
            feature = "test-unit-industry",
            feature = "test-unit-render"
        ))
    ),
    allow(dead_code, unused_imports)
)]

pub mod capability;
pub mod content;
pub mod core;
pub mod crafting;
pub mod electrical;
pub mod energy;
pub mod equipment;
pub mod fluid;
pub mod geology;
pub mod inventory;
pub mod labor;
pub mod maintenance;
pub mod material;
pub mod matter;
pub mod mechanical;
pub mod mining;
pub mod ore_processing;
pub mod persistence;
pub mod production;
pub mod registry;
pub mod shader;
pub mod simulation;
pub mod spatial;
pub mod structural;
pub mod survival;
pub mod texture;
pub mod thermal;

pub use content::build_registries;
pub use core::state::AppState;
pub use core::time::{SimulationTick, TickSpan, WorldSeed};
pub use registry::Registries;
pub use simulation::{TickError, TickOutcome, advance_tick};
