//! Additive, matter-conserving upgrades of existing equipment instances.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, StockpileId, StockpileStoredMassChange, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};
use crate::registry::Registries;

use super::state::EquipmentUpgradeMutation;
use super::{EquipmentDefinitionId, EquipmentId, EquipmentOccupancy, equipment_occupancy};

mod errors;

pub use errors::{EquipmentUpgradeCommitError, EquipmentUpgradeError};

fn validation_occupancy_error(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentUpgradeError> {
    equipment_occupancy(state, equipment).map(|occupancy| match occupancy {
        EquipmentOccupancy::Production { job, release } => {
            EquipmentUpgradeError::EquipmentBusyProduction {
                equipment,
                job,
                release,
            }
        }
        EquipmentOccupancy::Mining { job } => {
            EquipmentUpgradeError::EquipmentBusyMining { equipment, job }
        }
        EquipmentOccupancy::ManualPower { .. } => {
            EquipmentUpgradeError::EquipmentBusyManualPower { equipment }
        }
        EquipmentOccupancy::Prospecting { completes_at } => {
            EquipmentUpgradeError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            }
        }
        EquipmentOccupancy::Maintenance { completes_at } => {
            EquipmentUpgradeError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            }
        }
    })
}

fn commit_occupancy_error(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentUpgradeCommitError> {
    equipment_occupancy(state, equipment).map(|occupancy| match occupancy {
        EquipmentOccupancy::Production { job, .. } => {
            EquipmentUpgradeCommitError::EquipmentBusyProduction { equipment, job }
        }
        EquipmentOccupancy::Mining { job } => {
            EquipmentUpgradeCommitError::EquipmentBusyMining { equipment, job }
        }
        EquipmentOccupancy::ManualPower { .. } => {
            EquipmentUpgradeCommitError::EquipmentBusyManualPower { equipment }
        }
        EquipmentOccupancy::Prospecting { completes_at } => {
            EquipmentUpgradeCommitError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            }
        }
        EquipmentOccupancy::Maintenance { completes_at } => {
            EquipmentUpgradeCommitError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            }
        }
    })
}

#[must_use]
pub struct ValidatedEquipmentUpgrade {
    equipment: EquipmentId,
    expected_definition: EquipmentDefinitionId,
    target_definition: EquipmentDefinitionId,
    expected_embodied_mass: Mass,
    target_embodied_mass: Mass,
    additions: Vec<ConsumedMaterialTrace>,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEquipmentUpgrade {
    pub fn commit(self, state: &mut AppState) -> Result<EquipmentId, EquipmentUpgradeCommitError> {
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(EquipmentUpgradeCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(EquipmentUpgradeCommitError::StaleEquipment {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        let record = state.equipment().get_equipment(self.equipment).ok_or(
            EquipmentUpgradeCommitError::UnknownEquipment {
                equipment: self.equipment,
            },
        )?;
        if record.definition() != self.expected_definition {
            return Err(EquipmentUpgradeCommitError::DefinitionChanged {
                equipment: self.equipment,
                expected: self.expected_definition,
                actual: record.definition(),
            });
        }
        if let Some(element) = record.supported_by() {
            return Err(EquipmentUpgradeCommitError::EquipmentMounted {
                equipment: self.equipment,
                element,
            });
        }
        if let Some(error) = commit_occupancy_error(state, self.equipment) {
            return Err(error);
        }
        self.egress.assert_matches_state(state.inventory());
        let mutation = EquipmentUpgradeMutation {
            equipment: self.equipment,
            expected_definition: self.expected_definition,
            target_definition: self.target_definition,
            expected_embodied_mass: self.expected_embodied_mass,
            target_embodied_mass: self.target_embodied_mass,
            additions: self.additions,
        };
        state.equipment().assert_upgrade_available(
            &mutation,
            self.expected_equipment_revision,
            self.next_equipment_revision,
        );
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EquipmentUpgradeCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.equipment_state_mut().apply_upgrade(
            mutation,
            self.expected_equipment_revision,
            self.next_equipment_revision,
        );
        Ok(self.equipment)
    }
}

/// Validates one authored additive upgrade of an existing, unmounted, idle equipment instance.
pub fn validate_upgrade_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    target: EquipmentDefinitionId,
    source: StockpileId,
) -> Result<ValidatedEquipmentUpgrade, EquipmentUpgradeError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentUpgradeError::UnknownEquipment { equipment })?;
    let target_definition = registries
        .equipment()
        .get_equipment(target)
        .ok_or(EquipmentUpgradeError::UnknownTargetDefinition { target })?;
    let upgrade = target_definition
        .upgrade_profile()
        .ok_or(EquipmentUpgradeError::NoUpgradeProfile { target })?;
    if record.definition() != upgrade.from() {
        return Err(EquipmentUpgradeError::WrongBaseDefinition {
            equipment,
            required: upgrade.from(),
            actual: record.definition(),
        });
    }
    if let Some(element) = record.supported_by() {
        return Err(EquipmentUpgradeError::EquipmentMounted { equipment, element });
    }
    if let Some(error) = validation_occupancy_error(state, equipment) {
        return Err(error);
    }

    let selection =
        validate_consumption_selection(state.inventory(), source, upgrade.additions().inputs())
            .map_err(|error| match error {
                crate::inventory::ConsumptionSelectionError::UnknownStockpile { stockpile } => {
                    EquipmentUpgradeError::UnknownSource { stockpile }
                }
                crate::inventory::ConsumptionSelectionError::InsufficientMass {
                    stockpile,
                    available,
                    requested,
                    ..
                } => EquipmentUpgradeError::InsufficientMaterial {
                    stockpile,
                    available,
                    required: requested,
                },
                crate::inventory::ConsumptionSelectionError::MassOverflow { stockpile } => {
                    EquipmentUpgradeError::SourceMassOverflow { stockpile }
                }
            })?;
    let additions = selection.consumed_inputs().to_vec();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    EquipmentUpgradeError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    EquipmentUpgradeError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EquipmentUpgradeError::UnknownSource { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EquipmentUpgradeError::SourceMassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EquipmentUpgradeError::StructuralLoad)?;
    let expected_equipment_revision = state.equipment().revision();
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .ok_or(EquipmentUpgradeError::EquipmentRevisionExhausted)?;

    Ok(ValidatedEquipmentUpgrade {
        equipment,
        expected_definition: record.definition(),
        target_definition: target,
        expected_embodied_mass: record.embodied_mass(),
        target_embodied_mass: target_definition.mass(),
        additions,
        expected_equipment_revision,
        next_equipment_revision,
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "upgrade_execution_tests.rs"]
mod tests;
