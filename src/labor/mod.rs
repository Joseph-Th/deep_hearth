//! Exclusive player-work ownership shared by manual crafting and extraction systems.

mod lifecycle;
mod state;
mod validation;

pub use lifecycle::PlayerWorkStartError;
pub use state::{PlayerWork, PlayerWorkState};
pub use validation::PlayerWorkValidationError;

pub(crate) use lifecycle::{
    ValidatedPlayerWorkStart, apply_player_work_tick, decide_player_work_tick,
    player_work_exertion, validate_player_work_start,
};
pub(crate) use validation::validate_loaded_player_work;
