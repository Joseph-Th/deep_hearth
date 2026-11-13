//! Work-admission formatting regressions against real public error chains.

use super::*;
use deep_hearth::core::time::{ControlTick, require_simulation_boundary};
use deep_hearth::crafting::{PlayerManualCraftError, StartManualCraftError};
use deep_hearth::geology::FieldProspectingStartError;
use deep_hearth::labor::PlayerWorkStartError;
use deep_hearth::mining::MiningStartError;
use deep_hearth::ore_processing::{
    StartManualComminutionError, StartManualConstituentSeparationError,
};
use deep_hearth::production::StartProcessError;

#[test]
fn nested_typed_boundaries_receive_boundary_instruction_without_false_positives() {
    let boundary = require_simulation_boundary(ControlTick::new(1)).unwrap_err();
    let desired = "This task must start on a simulation boundary. Run 'wait boundary', then retry.";
    for error in [
        work_start_error(FieldProspectingStartError::Work(
            PlayerWorkStartError::SimulationBoundary(boundary),
        )),
        work_start_error(MiningStartError::Work(
            PlayerWorkStartError::SimulationBoundary(boundary),
        )),
        work_start_error(StartManualComminutionError::Process(
            StartProcessError::SimulationBoundary(boundary),
        )),
        work_start_error(StartManualConstituentSeparationError::Work(
            PlayerWorkStartError::SimulationBoundary(boundary),
        )),
        work_start_error(PlayerManualCraftError::Start(StartManualCraftError::Work(
            PlayerWorkStartError::SimulationBoundary(boundary),
        ))),
    ] {
        assert_eq!(error, desired);
    }

    assert_eq!(
        work_start_error(StartManualComminutionError::PlayerDead),
        "dead player cannot start manual comminution"
    );
    let misleading = "slow-scheduled work cannot begin: this is an unrelated error";
    assert_eq!(
        work_start_error(std::io::Error::other(misleading)),
        misleading,
        "displayed text must not be mistaken for a typed scheduling failure"
    );
}
