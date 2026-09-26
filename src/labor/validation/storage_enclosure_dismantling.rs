//! Trusted-load replay validation for direct storage-enclosure dismantling labor.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, StockpileEnclosureRecord, StockpileRecord,
    StorageDefinition, StorageEnclosureDismantlingError, validate_reserved_material_ingress,
    validate_stockpile_storage, validate_storage_dismantling_target_for_completion,
};
use crate::labor::StorageEnclosureDismantlingWork;
use crate::logistics::validate_player_stockpile_access;
use crate::registry::Registries;

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};

fn validate_work_identity(
    active_jobs: &ActivePlayerJobs,
    work: &StorageEnclosureDismantlingWork,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::StorageDismantlingResourceDoubleBooked);
    }
    if work.target() == work.recovery_destination() {
        return Err(PlayerWorkValidationError::StorageDismantlingRecoveryIsTarget);
    }
    Ok(())
}

fn validate_target<'a>(
    state: &'a AppState,
    work: &StorageEnclosureDismantlingWork,
) -> Result<(&'a StockpileRecord, &'a StockpileEnclosureRecord), PlayerWorkValidationError> {
    let target = state
        .inventory()
        .get_stockpile(work.target())
        .ok_or(PlayerWorkValidationError::StorageDismantlingTargetMissing)?;
    let enclosure = target
        .enclosure()
        .ok_or(PlayerWorkValidationError::StorageDismantlingEnclosureMissing)?;
    if enclosure.definition() != work.definition() {
        return Err(PlayerWorkValidationError::StorageDismantlingDefinitionMismatch);
    }
    if enclosure.created_at() != work.enclosure_created_at() {
        return Err(PlayerWorkValidationError::StorageDismantlingEnclosureIdentityMismatch);
    }
    if enclosure.embodied_mass() != work.recovered_mass() {
        return Err(PlayerWorkValidationError::StorageDismantlingRecoveredMassMismatch);
    }
    if target.supported_by().is_some() {
        return Err(PlayerWorkValidationError::StorageDismantlingTargetMounted);
    }
    if !target.reserved_inbound().is_zero() {
        return Err(PlayerWorkValidationError::StorageDismantlingTargetReservedInbound);
    }
    Ok((target, enclosure))
}

fn validate_recovery_destination(
    registries: &Registries,
    state: &AppState,
    enclosure: &StockpileEnclosureRecord,
    work: &StorageEnclosureDismantlingWork,
) -> Result<(), PlayerWorkValidationError> {
    let recovery = state
        .inventory()
        .get_stockpile(work.recovery_destination())
        .ok_or(PlayerWorkValidationError::StorageDismantlingRecoveryMissing)?;
    if recovery.supported_by().is_some() {
        return Err(PlayerWorkValidationError::StorageDismantlingRecoveryMounted);
    }
    for trace in enclosure.embodied_material() {
        let profile = trace.profile();
        validate_stockpile_storage(
            registries,
            recovery,
            recovery.id(),
            profile.commodity(),
            profile.composition(),
            profile.temperature(),
            profile.particle_size_distribution(),
        )
        .map_err(PlayerWorkValidationError::StorageDismantlingRecoveryStorage)?;
    }
    Ok(())
}

fn validate_definition_and_schedule<'a>(
    registries: &'a Registries,
    state: &AppState,
    target: &StockpileRecord,
    work: &StorageEnclosureDismantlingWork,
) -> Result<(&'a StorageDefinition, TickSpan), PlayerWorkValidationError> {
    let definition = registries
        .storage()
        .get(work.definition())
        .ok_or(PlayerWorkValidationError::StorageDismantlingDefinitionMissing)?;
    if target.storage_profile() != definition.storage_profile() {
        return Err(PlayerWorkValidationError::StorageDismantlingStorageProfileMismatch);
    }
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::StorageDismantlingScheduleInvalid)?;
    if schedule.duration != definition.dismantle_duration() {
        return Err(PlayerWorkValidationError::StorageDismantlingDurationMismatch);
    }
    Ok((definition, schedule.remaining))
}

fn validate_completion_replay(
    registries: &Registries,
    state: &AppState,
    work: &StorageEnclosureDismantlingWork,
) -> Result<(), PlayerWorkValidationError> {
    validate_storage_dismantling_target_for_completion(
        registries,
        state.inventory(),
        work.target(),
        work.completes_at(),
    )
    .map_err(|error| match error {
        StorageEnclosureDismantlingError::TargetContentsIncompatible { lot, .. } => {
            PlayerWorkValidationError::StorageDismantlingTargetContentsIncompatible { lot }
        }
        StorageEnclosureDismantlingError::Access(_)
        | StorageEnclosureDismantlingError::UnknownTarget { .. }
        | StorageEnclosureDismantlingError::NotEnclosed { .. }
        | StorageEnclosureDismantlingError::UnknownDefinition { .. }
        | StorageEnclosureDismantlingError::TargetMounted { .. }
        | StorageEnclosureDismantlingError::TargetHasReservedInbound { .. }
        | StorageEnclosureDismantlingError::UnknownRecoveryDestination { .. }
        | StorageEnclosureDismantlingError::RecoveryDestinationIsTarget { .. }
        | StorageEnclosureDismantlingError::RecoveryDestinationMounted { .. }
        | StorageEnclosureDismantlingError::RecoveryDestinationStorage(_)
        | StorageEnclosureDismantlingError::RecoveryCapacityExceeded { .. }
        | StorageEnclosureDismantlingError::RecoveryMassOverflow { .. }
        | StorageEnclosureDismantlingError::RecoveryLotIdExhausted
        | StorageEnclosureDismantlingError::InventoryRevisionExhausted
        | StorageEnclosureDismantlingError::CompletionTickOverflow { .. }
        | StorageEnclosureDismantlingError::PlayerWork(_) => unreachable!(
            "storage dismantling trusted-load target was fully checked before completion replay"
        ),
    })?;

    let enclosure = state
        .inventory()
        .get_stockpile(work.target())
        .and_then(StockpileRecord::enclosure)
        .unwrap_or_else(|| {
            unreachable!("storage dismantling target and enclosure were validated before ingress")
        });
    let ingress = validate_reserved_material_ingress(
        registries,
        state.inventory(),
        work.recovery_destination(),
        enclosure
            .embodied_material()
            .iter()
            .map(MaterialIngressEntry::from_consumed_trace),
        work.completes_at(),
        work.recovered_mass(),
    )
    .map_err(|error| match error {
        MaterialIngressError::LotIdExhausted => {
            PlayerWorkValidationError::StorageDismantlingRecoveryLotIdExhausted
        }
        MaterialIngressError::RevisionExhausted => {
            PlayerWorkValidationError::StorageDismantlingInventoryRevisionExhausted
        }
        MaterialIngressError::Empty
        | MaterialIngressError::UnknownStockpile { .. }
        | MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::Storage(_)
        | MaterialIngressError::ProvenanceInFuture { .. }
        | MaterialIngressError::MassOverflow { .. }
        | MaterialIngressError::CapacityExceeded { .. }
        | MaterialIngressError::ReservationMismatch { .. } => unreachable!(
            "storage dismantling trusted-load ingress facts were validated before completion replay: {error:?}"
        ),
    })?;
    ingress
        .next_revision()
        .checked_add(1)
        .ok_or(PlayerWorkValidationError::StorageDismantlingInventoryRevisionExhausted)?;
    Ok(())
}

pub(super) fn validate_storage_enclosure_dismantling_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: StorageEnclosureDismantlingWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    validate_work_identity(active_jobs, &work)?;
    validate_player_stockpile_access(state, work.target())
        .map_err(PlayerWorkValidationError::StorageDismantlingAccess)?;
    validate_player_stockpile_access(state, work.recovery_destination())
        .map_err(PlayerWorkValidationError::StorageDismantlingAccess)?;
    let (target, enclosure) = validate_target(state, &work)?;
    validate_recovery_destination(registries, state, enclosure, &work)?;
    let (definition, remaining) =
        validate_definition_and_schedule(registries, state, target, &work)?;
    validate_completion_replay(registries, state, &work)?;
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        definition.dismantle_exertion(),
        remaining,
    )
}
