//! Exclusive player-work ownership shared by manual crafting and extraction systems.

mod definitions;
mod lifecycle;
mod power_execution;
mod power_physics;
mod state;
mod validation;
mod work_resources;

pub use definitions::{LaborRegistry, ManualPowerDefinition, ManualPowerMethodId};
pub use lifecycle::{PlayerWorkCommitError, PlayerWorkStartError};
pub use power_execution::{
    ManualPowerCommitError, ManualPowerError, ManualPowerOutcome, ManualPowerRequest,
    ValidatedManualPowerStart, validate_start_manual_power,
};
pub use state::{ManualPowerWork, PlayerWork, PlayerWorkState};
pub use validation::PlayerWorkValidationError;

pub(crate) use lifecycle::{
    ValidatedPlayerWorkStart, apply_player_work_tick, decide_player_work_tick,
    player_work_exertion, validate_player_work_start,
};
pub(crate) use power_execution::{
    ManualPowerTickError, apply_manual_power_tick, decide_manual_power_tick,
};
pub(crate) use validation::validate_loaded_player_work;
pub(crate) use work_resources::{
    PlayerWorkResourceBudgetError, calculate_player_work_resource_budget,
};
