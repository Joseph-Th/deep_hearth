//! Trusted-load replay validation for direct storage-enclosure dismantling labor.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::{
    StorageEnclosureDismantlingError, validate_storage_dismantling_target_for_completion,
};
use crate::labor::StorageEnclosureDismantlingWork;
use crate::registry::Registries;

use super::{ActivePlayerJobs, PlayerWorkValidationError, validate_remaining_resources};

pub(super) fn validate_storage_enclosure_dismantling_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: StorageEnclosureDismantlingWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::StorageDismantlingResourceDoubleBooked);
    }
    if work.target() == work.recovery_destination() {
        return Err(PlayerWorkValidationError::StorageDismantlingRecoveryIsTarget);
    }

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

    let recovery = state
        .inventory()
        .get_stockpile(work.recovery_destination())
        .ok_or(PlayerWorkValidationError::StorageDismantlingRecoveryMissing)?;
    if recovery.supported_by().is_some() {
        return Err(PlayerWorkValidationError::StorageDismantlingRecoveryMounted);
    }

    let definition = registries
        .storage()
        .get(work.definition())
        .ok_or(PlayerWorkValidationError::StorageDismantlingDefinitionMissing)?;
    if target.storage_profile() != definition.storage_profile() {
        return Err(PlayerWorkValidationError::StorageDismantlingStorageProfileMismatch);
    }
    if work.started_at() > state.tick()
        || work.completes_at() <= state.tick()
        || work.completes_at() <= work.started_at()
    {
        return Err(PlayerWorkValidationError::StorageDismantlingScheduleInvalid);
    }
    let actual_duration = TickSpan::new(work.completes_at().value() - work.started_at().value());
    if actual_duration != definition.dismantle_duration() {
        return Err(PlayerWorkValidationError::StorageDismantlingDurationMismatch);
    }
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
        StorageEnclosureDismantlingError::StorageHistoryOverflow { lot } => {
            PlayerWorkValidationError::StorageDismantlingStorageHistoryOverflow { lot }
        }
        _ => unreachable!(
            "storage dismantling trusted-load target was fully checked before completion replay"
        ),
    })?;
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        definition.dismantle_exertion(),
        TickSpan::new(work.completes_at().value() - state.tick().value()),
    )
}
