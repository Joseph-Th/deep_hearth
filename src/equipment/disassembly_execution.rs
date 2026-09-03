//! Conserved recovery of assembled equipment.
//!
//! Pristine equipment reverses assembly exactly. Worn equipment with an authored recovery form is
//! destructively decommissioned into same-material scrap so wear cannot be erased and failed tools do
//! not permanently trap matter. Equipment without an authored worn-recovery policy remains intact.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId,
    StockpileStoredMassChange, ValidatedMaterialIngress, ValidatedStockpileStructuralLoad,
    apply_material_ingress, validate_material_ingress, validate_stockpile_stored_mass_changes,
};
use crate::maintenance::Condition;
use crate::registry::Registries;

use super::{EquipmentId, EquipmentOccupancy, equipment_occupancy};

mod errors;

pub use errors::{EquipmentDisassemblyCommitError, EquipmentDisassemblyError};

fn validation_occupancy_error(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentDisassemblyError> {
    equipment_occupancy(state, equipment).map(|occupancy| match occupancy {
        EquipmentOccupancy::Production { job, release } => {
            EquipmentDisassemblyError::EquipmentBusyProduction {
                equipment,
                job,
                release,
            }
        }
        EquipmentOccupancy::Mining { job } => {
            EquipmentDisassemblyError::EquipmentBusyMining { equipment, job }
        }
        EquipmentOccupancy::ManualPower { .. } => {
            EquipmentDisassemblyError::EquipmentBusyManualPower { equipment }
        }
        EquipmentOccupancy::Prospecting { completes_at } => {
            EquipmentDisassemblyError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            }
        }
        EquipmentOccupancy::Maintenance { completes_at } => {
            EquipmentDisassemblyError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            }
        }
    })
}

fn commit_occupancy_error(
    equipment: EquipmentId,
    occupancy: EquipmentOccupancy,
) -> EquipmentDisassemblyCommitError {
    match occupancy {
        EquipmentOccupancy::Production { job, .. } => {
            EquipmentDisassemblyCommitError::EquipmentBusyProduction { equipment, job }
        }
        EquipmentOccupancy::Mining { job } => {
            EquipmentDisassemblyCommitError::EquipmentBusyMining { equipment, job }
        }
        EquipmentOccupancy::ManualPower { .. } => {
            EquipmentDisassemblyCommitError::EquipmentBusyManualPower { equipment }
        }
        EquipmentOccupancy::Prospecting { completes_at } => {
            EquipmentDisassemblyCommitError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            }
        }
        EquipmentOccupancy::Maintenance { completes_at } => {
            EquipmentDisassemblyCommitError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            }
        }
    }
}

fn map_ingress_error(
    equipment: EquipmentId,
    error: MaterialIngressError,
) -> EquipmentDisassemblyError {
    match error {
        MaterialIngressError::Empty => EquipmentDisassemblyError::NoEmbodiedMatter { equipment },
        MaterialIngressError::UnknownStockpile { stockpile } => {
            EquipmentDisassemblyError::UnknownDestination { stockpile }
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            EquipmentDisassemblyError::DestinationMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => EquipmentDisassemblyError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => EquipmentDisassemblyError::LotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            EquipmentDisassemblyError::InventoryRevisionExhausted
        }
        MaterialIngressError::Storage(error) => {
            EquipmentDisassemblyError::DestinationStorage(error)
        }
        MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::InvalidProvenance
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            EquipmentDisassemblyError::InvalidEmbodiedMatter { equipment }
        }
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentDisassemblyOutcome {
    recovered_lots: Vec<MaterialLotId>,
}

impl EquipmentDisassemblyOutcome {
    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

#[must_use]
pub struct ValidatedEquipmentDisassembly {
    equipment: EquipmentId,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    expected_condition: Condition,
    expected_embodied_mass: Mass,
    ingress: ValidatedMaterialIngress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEquipmentDisassembly {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EquipmentDisassemblyOutcome, EquipmentDisassemblyCommitError> {
        if state.inventory().revision() != self.ingress.expected_revision() {
            return Err(EquipmentDisassemblyCommitError::StaleInventory {
                expected: self.ingress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(EquipmentDisassemblyCommitError::StaleEquipment {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        let record = state.equipment().get_equipment(self.equipment).ok_or(
            EquipmentDisassemblyCommitError::UnknownEquipment {
                equipment: self.equipment,
            },
        )?;
        if record.condition() != self.expected_condition
            || record.embodied_mass() != self.expected_embodied_mass
        {
            return Err(EquipmentDisassemblyCommitError::EquipmentChanged {
                equipment: self.equipment,
            });
        }
        let occupancy = equipment_occupancy(state, self.equipment);
        if let Some(
            occupancy @ (EquipmentOccupancy::Prospecting { .. }
            | EquipmentOccupancy::Maintenance { .. }),
        ) = occupancy
        {
            return Err(commit_occupancy_error(self.equipment, occupancy));
        }
        if let Some(element) = record.supported_by() {
            return Err(EquipmentDisassemblyCommitError::EquipmentMounted {
                equipment: self.equipment,
                element,
            });
        }
        if let Some(occupancy) = occupancy {
            return Err(commit_occupancy_error(self.equipment, occupancy));
        }
        self.ingress.assert_matches_state(state.inventory());
        state.equipment().assert_removal_available(
            self.equipment,
            self.expected_equipment_revision,
            self.next_equipment_revision,
        );
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EquipmentDisassemblyCommitError::Structure)?;
        }
        state.equipment_state_mut().remove_equipment(
            self.equipment,
            self.expected_equipment_revision,
            self.next_equipment_revision,
        );
        let recovered_lots = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        Ok(EquipmentDisassemblyOutcome { recovered_lots })
    }
}

/// Recovers idle, unmounted assembled equipment without allowing wear to reset into pristine parts.
pub fn validate_disassemble_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    destination: StockpileId,
) -> Result<ValidatedEquipmentDisassembly, EquipmentDisassemblyError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentDisassemblyError::UnknownEquipment { equipment })?;
    if record.embodied_mass().is_zero() || record.embodied_material().is_empty() {
        return Err(EquipmentDisassemblyError::NoEmbodiedMatter { equipment });
    }
    let worn_recovery_form = if record.condition() == Condition::PRISTINE {
        None
    } else {
        let definition = registries
            .equipment()
            .get_equipment(record.definition())
            .ok_or(EquipmentDisassemblyError::InvalidEmbodiedMatter { equipment })?;
        Some(definition.worn_recovery_form().ok_or(
            EquipmentDisassemblyError::WornRecoveryUnavailable {
                equipment,
                condition: record.condition(),
            },
        )?)
    };
    if let Some(element) = record.supported_by() {
        return Err(EquipmentDisassemblyError::EquipmentMounted { equipment, element });
    }
    if let Some(error) = validation_occupancy_error(state, equipment) {
        return Err(error);
    }

    let entries = record
        .embodied_material()
        .iter()
        .map(|trace| match worn_recovery_form {
            Some(form) => MaterialIngressEntry::from_reformed_consumed_trace(trace, form),
            None => MaterialIngressEntry::from_consumed_trace(trace),
        })
        .collect::<Vec<_>>();
    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        destination,
        entries,
        state.tick(),
    )
    .map_err(|error| map_ingress_error(equipment, error))?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        EquipmentDisassemblyError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    let destination_after = destination_record
        .stored_mass()
        .checked_add(record.embodied_mass())
        .ok_or(EquipmentDisassemblyError::DestinationMassOverflow {
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
    .map_err(EquipmentDisassemblyError::StoredMatterLoad)?;
    let expected_equipment_revision = state.equipment().revision();
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .ok_or(EquipmentDisassemblyError::EquipmentRevisionExhausted)?;

    Ok(ValidatedEquipmentDisassembly {
        equipment,
        expected_equipment_revision,
        next_equipment_revision,
        expected_condition: record.condition(),
        expected_embodied_mass: record.embodied_mass(),
        ingress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "disassembly_execution_tests.rs"]
mod tests;
