//! Trusted-load validation of embodied structural construction and recovery.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::labor::{StructuralConstructionWork, StructuralDismantlingWork};
use crate::registry::Registries;
use crate::structural::{
    StructuralLifecycle, bounds_are_within_player_reach, calculate_prismatic_material_mass_ceiling,
};

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};

pub(super) fn validate_structural_dismantling_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: StructuralDismantlingWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let method = registries
        .labor()
        .get_structural_construction(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::StructuralDismantlingMethodMissing)?;
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerWorkValidationError::StructuralDismantlingElementNotLocal)?;
    let record = state
        .structures()
        .get_element(work.element())
        .ok_or(PlayerWorkValidationError::StructuralDismantlingElementMissing)?;
    if record.embodied_mass().is_zero() {
        return Err(PlayerWorkValidationError::StructuralDismantlingElementNotRecoverable);
    }
    if !bounds_are_within_player_reach(registries, state, record.bounds()) {
        return Err(PlayerWorkValidationError::StructuralDismantlingElementNotLocal);
    }
    if state.structures().has_dependents(work.element()) {
        return Err(PlayerWorkValidationError::StructuralDismantlingElementHasDependents);
    }
    if record.loads().any(|(kind, load)| {
        kind != crate::structural::StructuralLoadKind::SelfWeight && !load.is_zero()
    }) {
        return Err(PlayerWorkValidationError::StructuralDismantlingElementHasExternalLoad);
    }
    if state
        .equipment()
        .supported_equipment(work.element())
        .next()
        .is_some()
        || state
            .inventory()
            .supported_stockpiles(work.element())
            .next()
            .is_some()
        || state
            .fluid()
            .supported_stores(work.element())
            .next()
            .is_some()
    {
        return Err(PlayerWorkValidationError::StructuralDismantlingElementSupportsResource);
    }
    if work.recovery_destination() != player.carried_stockpile()
        || work.recovered_mass() != record.embodied_mass()
    {
        return Err(PlayerWorkValidationError::StructuralDismantlingRecoveryMismatch);
    }
    let recovery = state
        .inventory()
        .get_stockpile(work.recovery_destination())
        .ok_or(PlayerWorkValidationError::StructuralDismantlingRecoveryMismatch)?;
    if recovery.reserved_inbound() < work.recovered_mass() {
        return Err(PlayerWorkValidationError::StructuralDismantlingRecoveryReservationMissing);
    }
    let trace_count = u64::try_from(record.embodied_material().len())
        .unwrap_or_else(|_| unreachable!("resident structural trace count fits u64"));
    if work.recovery_parcels() != trace_count {
        return Err(PlayerWorkValidationError::StructuralDismantlingLotDemandMismatch);
    }
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::StructuralDismantlingScheduleInvalid)?;
    if method.duration_for_mass(work.recovered_mass()) != Some(schedule.duration) {
        return Err(PlayerWorkValidationError::StructuralDismantlingDurationMismatch);
    }
    if !state.can_spend_inventory_revisions(0) {
        return Err(PlayerWorkValidationError::StructuralDismantlingInventoryRevisionExhausted);
    }
    if !state.can_spend_structure_revisions(0) {
        return Err(PlayerWorkValidationError::StructuralDismantlingStructureRevisionExhausted);
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

pub(super) fn validate_structural_construction_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: StructuralConstructionWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let method = registries
        .labor()
        .get_structural_construction(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::StructuralConstructionMethodMissing)?;
    if state.logistics().player().is_none() {
        return Err(PlayerWorkValidationError::StructuralConstructionPlayerNotLocated);
    }
    let record = state
        .structures()
        .get_element(work.element())
        .ok_or(PlayerWorkValidationError::StructuralConstructionElementMissing)?;
    if record.lifecycle() != StructuralLifecycle::Planned {
        return Err(PlayerWorkValidationError::StructuralConstructionElementNotPlanned);
    }
    if !record.is_grounded()
        && !state
            .structures()
            .supports(work.element())
            .is_some_and(|supports| {
                supports
                    .into_iter()
                    .any(|support| state.structures().get_element(support).is_some())
            })
    {
        return Err(PlayerWorkValidationError::StructuralConstructionElementUnsupported);
    }
    if !bounds_are_within_player_reach(registries, state, record.bounds()) {
        return Err(PlayerWorkValidationError::StructuralConstructionPlayerRemote);
    }
    let required = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        record.material(),
        record.cross_section(),
        record.length(),
    )
    .map_err(|_| PlayerWorkValidationError::StructuralConstructionElementNotMaterialized)?;
    if required.is_zero() || record.embodied_mass() != required {
        return Err(PlayerWorkValidationError::StructuralConstructionElementNotMaterialized);
    }
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::StructuralConstructionScheduleInvalid)?;
    if method.duration_for_mass(required) != Some(schedule.duration) {
        return Err(PlayerWorkValidationError::StructuralConstructionDurationMismatch);
    }
    if !state.can_spend_structure_revisions(0) {
        return Err(PlayerWorkValidationError::StructuralConstructionRevisionExhausted);
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
