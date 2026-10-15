//! Trusted-load replay validation for active direct block excavation.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use crate::equipment::{EquipmentOccupancy, equipment_occupancy};
use crate::inventory::validate_stockpile_storage;
use crate::labor::{BlockExcavationOutputCustody, BlockExcavationWork};
use crate::logistics::{validate_player_equipment_access, validate_player_stockpile_access};
use crate::material::MaterialLotSpec;
use crate::mining::resolve_mining_physics;
use crate::registry::Registries;
use crate::survival::{SurvivalExertion, control_exertion_between};

use super::{ActivePlayerJobs, PlayerWorkValidationError, calculate_player_work_resource_budget};

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
    let current = state.control_tick();
    if work.started_at() > current || work.completes_at() <= current {
        return Err(PlayerWorkValidationError::BlockExcavationScheduleInvalid);
    }
    let duration_control_ticks = work
        .duration_control_ticks()
        .ok_or(PlayerWorkValidationError::BlockExcavationScheduleInvalid)?;
    let expected_control_ticks = physics
        .duration()
        .value()
        .checked_mul(CONTROL_TICKS_PER_SIMULATION_TICK)
        .ok_or(PlayerWorkValidationError::BlockExcavationDurationMismatch)?;
    if duration_control_ticks != expected_control_ticks {
        return Err(PlayerWorkValidationError::BlockExcavationDurationMismatch);
    }
    let elapsed_control_ticks = current
        .checked_steps_since(work.started_at())
        .ok_or(PlayerWorkValidationError::BlockExcavationScheduleInvalid)?;
    let remaining_exertion = control_exertion_between(
        method.exertion(),
        elapsed_control_ticks,
        duration_control_ticks,
    )
    .ok_or(PlayerWorkValidationError::MetabolicCostOverflow)?;
    let slow_boundaries_remaining = work
        .completes_at()
        .simulation_tick()
        .value()
        .checked_sub(current.simulation_tick().value())
        .ok_or(PlayerWorkValidationError::BlockExcavationScheduleInvalid)?;
    let basal = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        SurvivalExertion::REST,
        TickSpan::new(slow_boundaries_remaining),
    )
    .map_err(|error| match error {
        crate::labor::PlayerWorkResourceBudgetError::EnergyOverflow => {
            PlayerWorkValidationError::MetabolicCostOverflow
        }
        crate::labor::PlayerWorkResourceBudgetError::HydrationOverflow => {
            PlayerWorkValidationError::HydrationCostOverflow
        }
    })?;
    let required_energy = basal
        .metabolic_energy()
        .checked_add(remaining_exertion.energy_cost_per_tick())
        .ok_or(PlayerWorkValidationError::MetabolicCostOverflow)?;
    let required_hydration = basal
        .hydration()
        .checked_add(remaining_exertion.hydration_loss_per_tick())
        .ok_or(PlayerWorkValidationError::HydrationCostOverflow)?;
    if available_energy < required_energy {
        return Err(PlayerWorkValidationError::InsufficientMetabolicEnergy {
            available: available_energy,
            required: required_energy,
        });
    }
    if available_hydration < required_hydration {
        return Err(PlayerWorkValidationError::InsufficientHydration {
            available: available_hydration,
            required: required_hydration,
        });
    }
    let survival_revision_demand = slow_boundaries_remaining
        .checked_add(u64::from(!work.completes_at().is_simulation_boundary()))
        .ok_or(PlayerWorkValidationError::SurvivalRevisionExhausted)?;
    if !state
        .survival()
        .can_advance_revision_by(survival_revision_demand)
    {
        return Err(PlayerWorkValidationError::SurvivalRevisionExhausted);
    }
    Ok(())
}
