//! Player-facing work admission errors without parsing formatted diagnostics.

use std::error::Error;

use deep_hearth::core::time::SimulationBoundaryError;
use deep_hearth::energy::{EnergySinkError, EnergySupplyError};
use deep_hearth::labor::PlayerWorkStartError;

use super::{fire, view};

pub(super) fn work_blocker(error: &PlayerWorkStartError) -> String {
    use PlayerWorkStartError as Work;
    match error {
        Work::SimulationBoundary(_) => {
            "This task must start on a simulation boundary. Run 'wait boundary', then retry."
        }
        Work::SurvivalNotInitialized => "The player has no initialized survival state.",
        Work::PlayerDead => "Cannot start work while dead.",
        Work::PlayerAirborne => "Stand on fixed ground before starting work.",
        Work::PlayerMoving => "Stop moving before starting work; use 'wait 10' to settle.",
        Work::PreparedStrikeActive => "Cancel the prepared strike before starting work.",
        Work::Busy { .. } => "Finish the current task with 'wait finish' before starting another.",
        Work::MetabolicCostOverflow { .. } | Work::HydrationCostOverflow { .. } => {
            "The requested work exceeds the supported exertion range."
        }
        Work::InsufficientMetabolicEnergy { available, required } => {
            return format!(
                "Not enough metabolic energy to complete the requested work ({} available, {} required). Eat first, or try a smaller batch if the task allows it.",
                fire::energy_label(*available),
                fire::energy_label(*required),
            );
        }
        Work::InsufficientHydration { available, required } => {
            return format!(
                "Not enough hydration to complete the requested work ({} mL available, {} mL required). Drink first, or try a smaller batch if the task allows it.",
                view::milliliters(*available),
                view::milliliters(*required),
            );
        }
        Work::RevisionExhausted | Work::SurvivalRevisionExhausted { .. } => {
            "The work cannot reserve another state change."
        }
    }
    .to_owned()
}

/// Canonical owners expose nested typed causes. Only an actual scheduling
/// boundary failure warrants the text client's boundary command suggestion;
/// unrelated errors retain their detailed owner diagnostic unchanged.
pub(super) fn work_start_error(error: impl Error + 'static) -> String {
    let mut cause: Option<&(dyn Error + 'static)> = Some(&error);
    while let Some(current) = cause {
        if let Some(work) = current.downcast_ref::<PlayerWorkStartError>() {
            return work_blocker(work);
        }
        if let Some(EnergySupplyError::InsufficientEnergy {
            available,
            requested,
            ..
        }) = current.downcast_ref::<EnergySupplyError>()
        {
            return format!(
                "Energy store has {} available; batch requires {}.",
                fire::energy_label(*available),
                fire::energy_label(*requested),
            );
        }
        if let Some(EnergySinkError::InsufficientCapacity {
            stored,
            requested,
            capacity,
            ..
        }) = current.downcast_ref::<EnergySinkError>()
        {
            return format!(
                "Heat sink would contain {} before completion; batch releases {}, exceeding capacity {}.",
                fire::energy_label(*stored),
                fire::energy_label(*requested),
                fire::energy_label(*capacity),
            );
        }
        if current.is::<SimulationBoundaryError>() {
            return "This task must start on a simulation boundary. Run 'wait boundary', then retry."
                .to_owned();
        }
        cause = current.source();
    }
    error.to_string()
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
