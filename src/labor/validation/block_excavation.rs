//! Trusted-load replay validation for active direct block excavation.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::equipment::{EquipmentOccupancy, equipment_occupancy};
use crate::inventory::validate_stockpile_storage;
use crate::labor::{BlockExcavationOutputCustody, BlockExcavationWork};
use crate::logistics::{validate_player_equipment_access, validate_player_stockpile_access};
use crate::material::MaterialLotSpec;
use crate::mining::resolve_mining_physics;
use crate::registry::Registries;

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};

pub(super) fn validate_block_excavation_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: BlockExcavationWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let method = registries
        .mining()
        .get_method(work.method())
        .ok_or(PlayerWorkValidationError::BlockExcavationMethodMissing)?;
    let current_state = state
        .world()
        .block_at(work.target())
        .ok_or(PlayerWorkValidationError::BlockExcavationTargetMissing)?;
    if current_state != work.state() {
        return Err(PlayerWorkValidationError::BlockExcavationTargetStateMismatch);
    }
    let block = registries
        .world()
        .block_for_state(work.state())
        .ok_or(PlayerWorkValidationError::BlockExcavationTargetStateMismatch)?;
    let excavation = block
        .excavation()
        .ok_or(PlayerWorkValidationError::BlockExcavationDefinitionMissing)?;
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), work.target())
        .ok_or(PlayerWorkValidationError::BlockExcavationTargetMissing)?;
    if source_mass != work.source_mass_before() {
        return Err(PlayerWorkValidationError::BlockExcavationSourceMassMismatch);
    }
    if work.excavated_mass().is_zero() || work.excavated_mass() > source_mass {
        return Err(PlayerWorkValidationError::BlockExcavationMassInvalid);
    }

    let trace = work.equipment_trace();
    let record = state
        .equipment()
        .get_equipment(work.equipment())
        .ok_or(PlayerWorkValidationError::BlockExcavationEquipmentMissing)?;
    validate_player_equipment_access(state, work.equipment())
        .map_err(PlayerWorkValidationError::BlockExcavationEquipmentAccess)?;
    if record.definition() != trace.definition() {
        return Err(PlayerWorkValidationError::BlockExcavationEquipmentDefinitionMismatch);
    }
    if record.condition() != trace.condition() {
        return Err(PlayerWorkValidationError::BlockExcavationEquipmentConditionMismatch);
    }
    if record.supported_by().is_some() {
        return Err(PlayerWorkValidationError::BlockExcavationEquipmentMounted);
    }
    if !matches!(
        equipment_occupancy(state, work.equipment()),
        Some(EquipmentOccupancy::BlockExcavation { completes_at })
            if completes_at == work.completes_at()
    ) {
        return Err(PlayerWorkValidationError::BlockExcavationEquipmentResourceDoubleBooked);
    }
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .ok_or(PlayerWorkValidationError::BlockExcavationEquipmentDefinitionMismatch)?;

    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerWorkValidationError::BlockExcavationPlayerNotLocated)?;
    match work.output() {
        BlockExcavationOutputCustody::Carried { stockpile } => {
            if player.carried_stockpile() != stockpile {
                return Err(PlayerWorkValidationError::BlockExcavationDestinationNotCarried);
            }
            validate_player_stockpile_access(state, stockpile)
                .map_err(PlayerWorkValidationError::BlockExcavationDestinationAccess)?;
            let destination = state
                .inventory()
                .get_stockpile(stockpile)
                .ok_or(PlayerWorkValidationError::BlockExcavationDestinationMissing)?;
            if destination.supported_by().is_some() {
                return Err(PlayerWorkValidationError::BlockExcavationDestinationMounted);
            }
            let output = MaterialLotSpec::new(
                excavation.fragmented_commodity(),
                work.excavated_mass(),
                excavation.temperature(),
            );
            validate_stockpile_storage(
                registries,
                destination,
                stockpile,
                output.commodity(),
                output.composition(),
                output.temperature(),
                output.particle_size_distribution(),
            )
            .map_err(PlayerWorkValidationError::BlockExcavationDestinationStorage)?;
            if destination.reserved_inbound() < work.excavated_mass() {
                return Err(PlayerWorkValidationError::BlockExcavationReservationMissing);
            }
        }
        BlockExcavationOutputCustody::Surface { position } => {
            if position != player.position() {
                return Err(
                    PlayerWorkValidationError::BlockExcavationSurfacePositionMismatch {
                        stored: position,
                        player: player.position(),
                    },
                );
            }
        }
    }

    let physics = resolve_mining_physics(
        registries.core().physical_tick_duration(),
        method,
        definition,
        trace.condition(),
        excavation.hardness(),
        work.excavated_mass(),
    )
    .map_err(|_| PlayerWorkValidationError::BlockExcavationPhysicsInvalid)?;
    if physics.condition_after() != work.condition_after() {
        return Err(PlayerWorkValidationError::BlockExcavationConditionOutcomeMismatch);
    }
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::BlockExcavationScheduleInvalid)?;
    if schedule.duration != physics.duration() {
        return Err(PlayerWorkValidationError::BlockExcavationDurationMismatch);
    }
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        method.exertion(),
        schedule.remaining,
    )
}
