//! Timed player dismantling of material-backed storage enclosures.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::labor::{
    PlayerWork, StorageEnclosureDismantlingWork, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use crate::registry::Registries;

use super::storage_validation::validate_stockpile_storage_profile;
use super::{
    InboundReservationError, InventoryState, MaterialIngressEntry, MaterialIngressError,
    StockpileEnclosureRecord, StockpileId, StockpileRecord, StockpileStorageProfile,
    StorageDefinitionId, ValidatedInboundReservation, validate_inbound_reservation,
    validate_material_ingress,
};

mod errors;
mod tick;

pub use errors::{StorageEnclosureDismantlingCommitError, StorageEnclosureDismantlingError};
pub use tick::StorageEnclosureDismantlingOutcome;
pub(crate) use tick::{
    StorageEnclosureDismantlingTickError, apply_storage_enclosure_dismantling_tick,
    decide_storage_enclosure_dismantling_tick,
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
        | MaterialIngressError::InvalidProvenance
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

/// Admission result for one dismantling interval. The enclosure remains installed until completion.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageEnclosureDismantlingStartOutcome {
    target: StockpileId,
    recovery_destination: StockpileId,
    definition: StorageDefinitionId,
    recovered_mass: crate::core::quantity::Mass,
    completes_at: SimulationTick,
}

impl StorageEnclosureDismantlingStartOutcome {
    #[must_use]
    pub const fn target(self) -> StockpileId {
        self.target
    }
    #[must_use]
    pub const fn recovery_destination(self) -> StockpileId {
        self.recovery_destination
    }
    #[must_use]
    pub const fn definition(self) -> StorageDefinitionId {
        self.definition
    }
    #[must_use]
    pub const fn recovered_mass(self) -> crate::core::quantity::Mass {
        self.recovered_mass
    }
    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Revision-bound proof that dismantling can reserve recovery capacity and player labor atomically.
#[must_use]
pub struct ValidatedStorageEnclosureDismantlingStart {
    target: StockpileId,
    definition: StorageDefinitionId,
    enclosure_created_at: SimulationTick,
    expected_profile: StockpileStorageProfile,
    reservation: ValidatedInboundReservation,
    work: StorageEnclosureDismantlingWork,
    player_work: ValidatedPlayerWorkStart,
}

impl ValidatedStorageEnclosureDismantlingStart {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StorageEnclosureDismantlingStartOutcome, StorageEnclosureDismantlingCommitError>
    {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.reservation.expected_revision() {
            return Err(
                StorageEnclosureDismantlingCommitError::StaleInventoryRevision {
                    expected: self.reservation.expected_revision(),
                    actual: actual_revision,
                },
            );
        }
        let target = state.inventory().get_stockpile(self.target).ok_or(
            StorageEnclosureDismantlingCommitError::UnknownTarget {
                stockpile: self.target,
            },
        )?;
        if target.storage_profile() != self.expected_profile {
            return Err(
                StorageEnclosureDismantlingCommitError::TargetProfileChanged {
                    stockpile: self.target,
                },
            );
        }
        let enclosure = target.enclosure().ok_or(
            StorageEnclosureDismantlingCommitError::TargetEnclosureChanged {
                stockpile: self.target,
            },
        )?;
        if enclosure.definition() != self.definition
            || enclosure.created_at() != self.enclosure_created_at
        {
            return Err(
                StorageEnclosureDismantlingCommitError::TargetEnclosureChanged {
                    stockpile: self.target,
                },
            );
        }
        self.player_work
            .precheck(state)
            .map_err(StorageEnclosureDismantlingCommitError::PlayerWork)?;
        self.reservation.assert_matches_state(state.inventory());
        self.reservation.apply(state.inventory_state_mut());
        self.player_work.apply(state);
        Ok(StorageEnclosureDismantlingStartOutcome {
            target: self.work.target(),
            recovery_destination: self.work.recovery_destination(),
            definition: self.work.definition(),
            recovered_mass: self.work.recovered_mass(),
            completes_at: self.work.completes_at(),
        })
    }
}

pub(crate) fn validate_storage_dismantling_target_for_completion(
    registries: &Registries,
    inventory: &InventoryState,
    target: StockpileId,
    at: SimulationTick,
) -> Result<(), StorageEnclosureDismantlingError> {
    let target_record = inventory
        .get_stockpile(target)
        .ok_or(StorageEnclosureDismantlingError::UnknownTarget { stockpile: target })?;
    let next_profile = StockpileStorageProfile::unbounded_solid_only();
    let source_preservation = target_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let destination_preservation = next_profile.preservation_multiplier_ppm();
    for lot in inventory.lot_ids(target) {
        let record = inventory
            .get_lot(lot)
            .unwrap_or_else(|| unreachable!("stockpile lot index references a live lot"));
        validate_stockpile_storage_profile(
            registries,
            next_profile,
            target,
            record.commodity(),
            record.composition(),
            record.temperature(),
            record.particle_size_distribution(),
        )
        .map_err(
            |error| StorageEnclosureDismantlingError::TargetContentsIncompatible { lot, error },
        )?;
        if record
            .storage_history()
            .transition_preservation(at, source_preservation, destination_preservation)
            .is_none()
        {
            return Err(StorageEnclosureDismantlingError::StorageHistoryOverflow { lot });
        }
    }
    Ok(())
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
    if state.inventory().revision().checked_add(3).is_none() {
        return Err(StorageEnclosureDismantlingError::InventoryRevisionExhausted);
    }
    let entries = enclosure
        .embodied_material()
        .iter()
        .map(MaterialIngressEntry::from_consumed_trace)
        .collect::<Vec<_>>();
    let _ = validate_material_ingress(
        registries,
        state.inventory(),
        recovery_destination,
        entries,
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
        reservation,
        work,
        player_work,
    })
}

#[cfg(test)]
#[path = "enclosure_dismantling_tests.rs"]
mod tests;
