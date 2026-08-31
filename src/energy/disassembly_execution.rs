//! Identity-preserving reversal of empty, material-backed energy-store construction.
//!
//! Dismantling is intentionally limited to empty, idle stores. Stored work remains an authoritative
//! resource and can never disappear merely because its container is being removed.

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId,
    StockpileStoredMassChange, ValidatedMaterialIngress, ValidatedStockpileStructuralLoad,
    apply_material_ingress, validate_material_ingress, validate_stockpile_stored_mass_changes,
};
use crate::registry::Registries;

use super::EnergyStoreId;

mod errors;

pub use errors::{EnergyStoreDisassemblyCommitError, EnergyStoreDisassemblyError};

fn map_ingress_error(
    store: EnergyStoreId,
    error: MaterialIngressError,
) -> EnergyStoreDisassemblyError {
    match error {
        MaterialIngressError::Empty => EnergyStoreDisassemblyError::NoEmbodiedMatter { store },
        MaterialIngressError::UnknownStockpile { stockpile } => {
            EnergyStoreDisassemblyError::UnknownDestination { stockpile }
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            EnergyStoreDisassemblyError::DestinationMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => EnergyStoreDisassemblyError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => EnergyStoreDisassemblyError::LotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            EnergyStoreDisassemblyError::InventoryRevisionExhausted
        }
        MaterialIngressError::Storage(error) => {
            EnergyStoreDisassemblyError::DestinationStorage(error)
        }
        MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::InvalidProvenance
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            EnergyStoreDisassemblyError::InvalidEmbodiedMatter { store }
        }
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyStoreDisassemblyOutcome {
    recovered_lots: Vec<MaterialLotId>,
}

impl EnergyStoreDisassemblyOutcome {
    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

#[must_use]
pub struct ValidatedEnergyStoreDisassembly {
    store: EnergyStoreId,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_embodied_mass: Mass,
    ingress: ValidatedMaterialIngress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEnergyStoreDisassembly {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EnergyStoreDisassemblyOutcome, EnergyStoreDisassemblyCommitError> {
        if state.inventory().revision() != self.ingress.expected_revision() {
            return Err(EnergyStoreDisassemblyCommitError::StaleInventory {
                expected: self.ingress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(EnergyStoreDisassemblyCommitError::StaleEnergy {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        let record = state
            .energy()
            .get_store(self.store)
            .ok_or(EnergyStoreDisassemblyCommitError::UnknownStore { store: self.store })?;
        if record.stored() != Energy::ZERO || record.embodied_mass() != self.expected_embodied_mass
        {
            return Err(EnergyStoreDisassemblyCommitError::StoreChanged { store: self.store });
        }
        if let Some(job) = state.production().get_energy_occupant(self.store) {
            return Err(EnergyStoreDisassemblyCommitError::StoreBusyProduction {
                store: self.store,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_energy_occupant(self.store)
            .is_some()
        {
            return Err(EnergyStoreDisassemblyCommitError::StoreBusyManualPower {
                store: self.store,
            });
        }
        self.ingress.assert_matches_state(state.inventory());
        state.energy().assert_removal_available(
            self.store,
            self.expected_energy_revision,
            self.next_energy_revision,
        );
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EnergyStoreDisassemblyCommitError::Structure)?;
        }
        state.energy_state_mut().remove_store(
            self.store,
            self.expected_energy_revision,
            self.next_energy_revision,
        );
        let recovered_lots = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        Ok(EnergyStoreDisassemblyOutcome { recovered_lots })
    }
}

/// Reverses construction of one empty, idle, material-backed energy store.
pub fn validate_disassemble_energy_store(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    destination: StockpileId,
) -> Result<ValidatedEnergyStoreDisassembly, EnergyStoreDisassemblyError> {
    let record = state
        .energy()
        .get_store(store)
        .ok_or(EnergyStoreDisassemblyError::UnknownStore { store })?;
    if record.embodied_mass().is_zero() || record.embodied_material().is_empty() {
        return Err(EnergyStoreDisassemblyError::NoEmbodiedMatter { store });
    }
    if record.stored() != Energy::ZERO {
        return Err(EnergyStoreDisassemblyError::StoreNotEmpty {
            store,
            stored: record.stored(),
        });
    }
    if let Some(job) = state.production().get_energy_occupant(store) {
        let release = state
            .production()
            .get_job(job)
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: energy occupancy references missing production job {}",
                    job.value()
                )
            })
            .occupancy_release();
        return Err(EnergyStoreDisassemblyError::StoreBusyProduction {
            store,
            job,
            release,
        });
    }
    if state
        .player_work()
        .get_manual_power_energy_occupant(store)
        .is_some()
    {
        return Err(EnergyStoreDisassemblyError::StoreBusyManualPower { store });
    }

    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        destination,
        record
            .embodied_material()
            .iter()
            .map(MaterialIngressEntry::from_consumed_trace),
        state.tick(),
    )
    .map_err(|error| map_ingress_error(store, error))?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        EnergyStoreDisassemblyError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    let destination_after = destination_record
        .stored_mass()
        .checked_add(record.embodied_mass())
        .ok_or(EnergyStoreDisassemblyError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(
            destination,
            destination_after,
        )],
    )
    .map_err(EnergyStoreDisassemblyError::StoredMatterLoad)?;
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .ok_or(EnergyStoreDisassemblyError::EnergyRevisionExhausted)?;

    Ok(ValidatedEnergyStoreDisassembly {
        store,
        expected_energy_revision,
        next_energy_revision,
        expected_embodied_mass: record.embodied_mass(),
        ingress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "disassembly_execution_tests.rs"]
mod tests;
