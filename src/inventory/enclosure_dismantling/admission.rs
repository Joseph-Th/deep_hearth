//! Read-only admission and reservation planning for storage-enclosure dismantling.

use crate::core::state::AppState;
use crate::labor::{PlayerWork, StorageEnclosureDismantlingWork, validate_player_work_start};
use crate::logistics::validate_player_stockpile_access;
use crate::registry::Registries;

use super::super::{
    InboundReservationError, MaterialIngressEntry, MaterialIngressError, StockpileEnclosureRecord,
    StockpileId, StockpileRecord, ValidatedInboundReservation, validate_inbound_reservation,
    validate_material_ingress,
};
use super::{
    StorageEnclosureDismantlingError, ValidatedStorageEnclosureDismantlingStart,
    validate_storage_dismantling_target_for_completion,
};

fn map_recovery_ingress_error(error: MaterialIngressError) -> StorageEnclosureDismantlingError {
    match error {
        MaterialIngressError::UnknownStockpile { stockpile } => {
            StorageEnclosureDismantlingError::UnknownRecoveryDestination { stockpile }
        }
        MaterialIngressError::Storage(error) => {
            StorageEnclosureDismantlingError::RecoveryDestinationStorage(error)
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            StorageEnclosureDismantlingError::RecoveryMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => StorageEnclosureDismantlingError::RecoveryCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => {
            StorageEnclosureDismantlingError::RecoveryLotIdExhausted
        }
        MaterialIngressError::RevisionExhausted => {
            StorageEnclosureDismantlingError::InventoryRevisionExhausted
        }
        MaterialIngressError::Empty
        | MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::ReservationMismatch { .. }
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            unreachable!("validated enclosure embodiment must remain valid material ingress")
        }
    }
}

fn map_reservation_error(error: InboundReservationError) -> StorageEnclosureDismantlingError {
    match error {
        InboundReservationError::UnknownStockpile { stockpile } => {
            StorageEnclosureDismantlingError::UnknownRecoveryDestination { stockpile }
        }
        InboundReservationError::MassOverflow { stockpile } => {
            StorageEnclosureDismantlingError::RecoveryMassOverflow { stockpile }
        }
        InboundReservationError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => StorageEnclosureDismantlingError::RecoveryCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        InboundReservationError::RevisionExhausted => {
            StorageEnclosureDismantlingError::InventoryRevisionExhausted
        }
    }
}

fn validate_dismantling_target(
    state: &AppState,
    target: StockpileId,
) -> Result<(&StockpileRecord, &StockpileEnclosureRecord), StorageEnclosureDismantlingError> {
    let target_record = state
        .inventory()
        .get_stockpile(target)
        .ok_or(StorageEnclosureDismantlingError::UnknownTarget { stockpile: target })?;
    let enclosure = target_record
        .enclosure()
        .ok_or(StorageEnclosureDismantlingError::NotEnclosed { stockpile: target })?;
    if let Some(element) = target_record.supported_by() {
        return Err(StorageEnclosureDismantlingError::TargetMounted {
            stockpile: target,
            element,
        });
    }
    if !target_record.reserved_inbound().is_zero() {
        return Err(StorageEnclosureDismantlingError::TargetHasReservedInbound {
            stockpile: target,
            reserved: target_record.reserved_inbound(),
        });
    }
    Ok((target_record, enclosure))
}

fn validate_dismantling_recovery_destination(
    state: &AppState,
    target: StockpileId,
    recovery_destination: StockpileId,
) -> Result<(), StorageEnclosureDismantlingError> {
    if recovery_destination == target {
        return Err(
            StorageEnclosureDismantlingError::RecoveryDestinationIsTarget { stockpile: target },
        );
    }
    let recovery_record = state
        .inventory()
        .get_stockpile(recovery_destination)
        .ok_or(
            StorageEnclosureDismantlingError::UnknownRecoveryDestination {
                stockpile: recovery_destination,
            },
        )?;
    if let Some(element) = recovery_record.supported_by() {
        return Err(
            StorageEnclosureDismantlingError::RecoveryDestinationMounted {
                stockpile: recovery_destination,
                element,
            },
        );
    }
    Ok(())
}

fn validate_dismantling_inventory_capacity(
    registries: &Registries,
    state: &AppState,
    enclosure: &StockpileEnclosureRecord,
    recovery_destination: StockpileId,
) -> Result<ValidatedInboundReservation, StorageEnclosureDismantlingError> {
    if !state.can_spend_inventory_revisions(3) {
        return Err(StorageEnclosureDismantlingError::InventoryRevisionExhausted);
    }
    let future_recovery_parcels = u64::try_from(enclosure.embodied_material().len())
        .unwrap_or_else(|_| unreachable!("resident enclosure trace count fits u64"));
    if !state
        .has_material_lot_id_headroom_from(state.inventory().next_lot_id(), future_recovery_parcels)
    {
        return Err(StorageEnclosureDismantlingError::RecoveryLotIdExhausted);
    }
    // Admission precheck only: the returned ingress plan is intentionally discarded. Full
    // ingress legality (containment, capacity, lot-ID space) must hold before reserving, while
    // the reservation below owns the capacity the completion stage re-validates against.
    let _ = validate_material_ingress(
        registries,
        state.inventory(),
        recovery_destination,
        enclosure
            .embodied_material()
            .iter()
            .map(MaterialIngressEntry::from_consumed_trace),
        state.tick(),
    )
    .map_err(map_recovery_ingress_error)?;
    validate_inbound_reservation(
        state.inventory(),
        recovery_destination,
        enclosure.embodied_mass(),
    )
    .map_err(map_reservation_error)
}

/// Starts dismantling one material-backed enclosure into a distinct, unmounted recovery stockpile.
///
/// Recovery capacity is reserved at admission, but the enclosure remains installed and continues
/// to preserve its contents until the final active-work tick. Exact embodied matter changes custody
/// only at completion.
pub fn validate_start_storage_enclosure_dismantling(
    registries: &Registries,
    state: &AppState,
    target: StockpileId,
    recovery_destination: StockpileId,
) -> Result<ValidatedStorageEnclosureDismantlingStart, StorageEnclosureDismantlingError> {
    validate_player_stockpile_access(state, target)
        .map_err(StorageEnclosureDismantlingError::Access)?;
    validate_player_stockpile_access(state, recovery_destination)
        .map_err(StorageEnclosureDismantlingError::Access)?;
    let (target_record, enclosure) = validate_dismantling_target(state, target)?;
    validate_dismantling_recovery_destination(state, target, recovery_destination)?;
    let definition = enclosure.definition();
    let definition_record = registries
        .storage()
        .get(definition)
        .ok_or(StorageEnclosureDismantlingError::UnknownDefinition { definition })?;
    let duration = definition_record.dismantle_duration();
    let completes_at = state.tick().checked_add_span(duration).ok_or(
        StorageEnclosureDismantlingError::CompletionTickOverflow {
            current: state.tick(),
            duration,
        },
    )?;
    validate_storage_dismantling_target_for_completion(
        registries,
        state.inventory(),
        target,
        completes_at,
    )?;
    let recovered_mass = enclosure.embodied_mass();
    let reservation = validate_dismantling_inventory_capacity(
        registries,
        state,
        enclosure,
        recovery_destination,
    )?;
    let work = StorageEnclosureDismantlingWork::new(
        target,
        recovery_destination,
        definition,
        enclosure.created_at(),
        recovered_mass,
        state.tick(),
        completes_at,
    );
    let player_work = validate_player_work_start(
        registries,
        state,
        PlayerWork::StorageEnclosureDismantling { work },
        duration,
        definition_record.dismantle_exertion(),
    )
    .map_err(StorageEnclosureDismantlingError::PlayerWork)?;
    Ok(ValidatedStorageEnclosureDismantlingStart {
        target,
        definition,
        enclosure_created_at: enclosure.created_at(),
        expected_profile: target_record.storage_profile(),
        expected_logistics_revision: state.logistics().revision(),
        reservation,
        work,
        player_work,
    })
}
