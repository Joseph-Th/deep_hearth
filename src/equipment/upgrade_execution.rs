//! Additive, matter-conserving upgrades of existing equipment instances.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, ConsumptionSelectionError, InventoryState, StockpileId,
    StockpileStoredMassChange, TemperatureLimitedSelectionError, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
    validate_temperature_limited_consumption_selection,
    validate_unreserved_stockpile_structural_load_headroom,
};
use crate::logistics::{validate_player_equipment_access, validate_player_stockpile_access};
use crate::material::{MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE, MaterialInputSpec};
use crate::player::{PlayerActionGuard, validate_optional_player_action};
use crate::registry::Registries;

use super::state::EquipmentUpgradeMutation;
use super::{EquipmentDefinitionId, EquipmentId, EquipmentOccupancy, equipment_occupancy};

mod errors;

pub use errors::{EquipmentUpgradeCommitError, EquipmentUpgradeError};

fn map_upgrade_selection_error(error: ConsumptionSelectionError) -> EquipmentUpgradeError {
    match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            EquipmentUpgradeError::UnknownSource { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => EquipmentUpgradeError::InsufficientMaterial {
            stockpile,
            commodity,
            available,
            required: requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            EquipmentUpgradeError::SourceMassOverflow { stockpile }
        }
    }
}

pub(super) fn validate_upgrade_material_selection(
    inventory: &InventoryState,
    source: StockpileId,
    inputs: &[MaterialInputSpec],
) -> Result<crate::inventory::ConsumptionSelection, EquipmentUpgradeError> {
    validate_temperature_limited_consumption_selection(
        inventory,
        source,
        inputs,
        MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
    )
    .map_err(|error| match error {
        TemperatureLimitedSelectionError::Selection(error) => map_upgrade_selection_error(error),
        TemperatureLimitedSelectionError::TooHot {
            commodity,
            temperature,
            maximum,
        } => EquipmentUpgradeError::UpgradeMaterialTooHot {
            commodity,
            temperature,
            maximum,
        },
    })
}

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
        EquipmentOccupancy::TreeFelling { completes_at } => {
            EquipmentUpgradeError::EquipmentBusyTreeFelling {
                equipment,
                completes_at,
            }
        }
        EquipmentOccupancy::BlockExcavation { completes_at } => {
            EquipmentUpgradeError::EquipmentBusyBlockExcavation {
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
        EquipmentOccupancy::TreeFelling { completes_at } => {
            EquipmentUpgradeCommitError::EquipmentBusyTreeFelling {
                equipment,
                completes_at,
            }
        }
        EquipmentOccupancy::BlockExcavation { completes_at } => {
            EquipmentUpgradeCommitError::EquipmentBusyBlockExcavation {
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
    player_action: PlayerActionGuard,
    equipment: EquipmentId,
    expected_definition: EquipmentDefinitionId,
    target_definition: EquipmentDefinitionId,
    expected_embodied_mass: Mass,
    target_embodied_mass: Mass,
    additions: Vec<ConsumedMaterialTrace>,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    expected_logistics_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEquipmentUpgrade {
    pub fn commit(self, state: &mut AppState) -> Result<EquipmentId, EquipmentUpgradeCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(EquipmentUpgradeCommitError::StaleLogistics {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        self.player_action.precheck(state).map_err(|stale| {
            EquipmentUpgradeCommitError::StaleSurvival {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        if state.logistics().player().is_some() && state.player_work().active().is_some() {
            return Err(EquipmentUpgradeCommitError::PlayerBecameBusy);
        }
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
    let player_action =
        validate_optional_player_action(state).map_err(|_| EquipmentUpgradeError::PlayerDead)?;
    if state.logistics().player().is_some() && state.player_work().active().is_some() {
        return Err(EquipmentUpgradeError::PlayerBusy);
    }
    validate_player_equipment_access(state, equipment)
        .map_err(EquipmentUpgradeError::EquipmentAccess)?;
    validate_player_stockpile_access(state, source).map_err(EquipmentUpgradeError::SourceAccess)?;
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
    if state.logistics().is_equipment_carried(equipment) {
        return Err(EquipmentUpgradeError::EquipmentCarried { equipment });
    }
    if let Some(error) = validation_occupancy_error(state, equipment) {
        return Err(error);
    }

    let selection = validate_upgrade_material_selection(
        state.inventory(),
        source,
        upgrade.additions().inputs(),
    )?;
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
    let source_after = egress.source_stored_mass_after(state.inventory());
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EquipmentUpgradeError::StructuralLoad)?;
    validate_unreserved_stockpile_structural_load_headroom(state, structural_load.as_ref())
        .map_err(EquipmentUpgradeError::StructuralLoad)?;
    let expected_equipment_revision = state.equipment().revision();
    if !state.can_spend_inventory_revisions(1) {
        return Err(EquipmentUpgradeError::InventoryRevisionExhausted);
    }
    if !state.can_spend_equipment_revisions(1) {
        return Err(EquipmentUpgradeError::EquipmentRevisionExhausted);
    }
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("equipment headroom check includes upgrade revision"));

    Ok(ValidatedEquipmentUpgrade {
        player_action,
        equipment,
        expected_definition: record.definition(),
        target_definition: target,
        expected_embodied_mass: record.embodied_mass(),
        target_embodied_mass: target_definition.mass(),
        additions,
        expected_equipment_revision,
        next_equipment_revision,
        expected_logistics_revision: state.logistics().revision(),
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "upgrade_execution_tests.rs"]
mod tests;
