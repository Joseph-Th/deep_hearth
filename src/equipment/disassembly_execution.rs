//! Conserved recovery of assembled equipment.
//!
//! Pristine equipment reverses assembly exactly. For component-maintained equipment, worn
//! disassembly preserves unrelated embodied components and reforms only the authored wear component
//! into its spent form. Worn equipment without component-replacement semantics remains intact.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialLotId, ValidatedMaterialIngress, ValidatedStockpileStructuralLoad,
    apply_material_ingress,
};
use crate::maintenance::Condition;

use super::{EquipmentId, EquipmentOccupancy, equipment_occupancy};

mod admission;
mod errors;

pub use admission::validate_disassemble_equipment;
pub use errors::{EquipmentDisassemblyCommitError, EquipmentDisassemblyError};

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

#[cfg(test)]
#[path = "disassembly_execution_tests.rs"]
mod tests;
