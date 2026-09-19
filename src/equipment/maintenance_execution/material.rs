//! Exact material exchange owned by equipment maintenance.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, MaterialReformCommitError, MaterialReformError, ValidatedMaterialEgress,
    ValidatedMaterialIngress, ValidatedMaterialReform, ValidatedStockpileStructuralLoad,
    apply_material_egress, apply_material_ingress, validate_material_reform_from_selection,
};
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::{EquipmentMaintenanceCommitError, EquipmentMaintenanceMaterialError};
use crate::equipment::maintenance_resolution::{
    EquipmentMaintenanceMaterialResolution, EquipmentMaintenanceResolution,
};
use crate::equipment::state::{
    EquipmentComponentMaintenanceMutation, EquipmentId, EquipmentRecord,
};

mod component;

use component::validate_component_exchange;

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ValidatedMaintenanceMaterial {
    Aggregate(ValidatedMaterialReform),
    Component {
        component: CommodityKey,
        replacement: Vec<ConsumedMaterialTrace>,
        egress: ValidatedMaterialEgress,
        worn_ingress: ValidatedMaterialIngress,
        structural: Option<ValidatedStockpileStructuralLoad>,
    },
}

impl ValidatedMaintenanceMaterial {
    pub(super) fn material_mass(&self) -> Mass {
        match self {
            Self::Aggregate(material) => material.total_mass(),
            Self::Component { egress, .. } => egress.total_consumed(),
        }
    }

    pub(super) fn commit(
        self,
        state: &mut AppState,
        equipment: EquipmentId,
        condition_before: crate::maintenance::Condition,
        expected_equipment_revision: u64,
        next_equipment_revision: u64,
    ) -> Result<(), EquipmentMaintenanceCommitError> {
        match self {
            Self::Aggregate(material) => {
                state.equipment().assert_maintenance_admission_available(
                    equipment,
                    condition_before,
                    expected_equipment_revision,
                    next_equipment_revision,
                );
                material.commit(state).map_err(map_reform_commit_error)?;
                state.equipment_state_mut().apply_maintenance_admission(
                    equipment,
                    condition_before,
                    expected_equipment_revision,
                    next_equipment_revision,
                );
            }
            Self::Component {
                component,
                replacement,
                egress,
                worn_ingress,
                structural,
            } => {
                let mutation = EquipmentComponentMaintenanceMutation {
                    equipment,
                    component,
                    condition_before,
                    replacement,
                };
                state.equipment().assert_component_maintenance_available(
                    &mutation,
                    expected_equipment_revision,
                    next_equipment_revision,
                );
                if state.inventory().revision() != egress.expected_revision() {
                    return Err(EquipmentMaintenanceCommitError::StaleInventoryRevision {
                        expected: egress.expected_revision(),
                        actual: state.inventory().revision(),
                    });
                }
                egress.assert_matches_state(state.inventory());
                assert_eq!(
                    state.inventory().revision().checked_add(1),
                    Some(worn_ingress.expected_revision()),
                    "maintenance spent-material ingress must follow replacement-material egress"
                );
                worn_ingress.assert_matches_state_after_egress(state.inventory(), &egress);
                if let Some(structural) = structural {
                    structural
                        .commit(state)
                        .map_err(EquipmentMaintenanceCommitError::Structure)?;
                }
                apply_material_egress(state.inventory_state_mut(), egress);
                state.equipment_state_mut().apply_component_maintenance(
                    mutation,
                    expected_equipment_revision,
                    next_equipment_revision,
                );
                apply_material_ingress(state.inventory_state_mut(), worn_ingress);
            }
        }
        Ok(())
    }
}

fn map_reform_error(error: MaterialReformError) -> EquipmentMaintenanceMaterialError {
    match error {
        MaterialReformError::StaleSelection { expected, actual } => {
            EquipmentMaintenanceMaterialError::StaleSelection { expected, actual }
        }
        MaterialReformError::UnknownDestination { stockpile } => {
            EquipmentMaintenanceMaterialError::UnknownSpentDestination { stockpile }
        }
        MaterialReformError::UnknownTargetMaterial { material } => {
            EquipmentMaintenanceMaterialError::UnknownSpentMaterial { material }
        }
        MaterialReformError::UnknownTargetForm { form } => {
            EquipmentMaintenanceMaterialError::UnknownSpentForm { form }
        }
        MaterialReformError::MaterialChanged { source, target } => {
            EquipmentMaintenanceMaterialError::SpentMaterialChanged { source, target }
        }
        MaterialReformError::PhaseChanged { source, target } => {
            EquipmentMaintenanceMaterialError::SpentPhaseChanged {
                replacement: source,
                spent: target,
            }
        }
        MaterialReformError::TargetUnchanged { commodity } => {
            EquipmentMaintenanceMaterialError::SpentFormUnchanged { commodity }
        }
        MaterialReformError::DestinationStorage(error) => {
            EquipmentMaintenanceMaterialError::SpentStorage(error)
        }
        MaterialReformError::DestinationMassOverflow { stockpile } => {
            EquipmentMaintenanceMaterialError::SpentMassOverflow { stockpile }
        }
        MaterialReformError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => EquipmentMaintenanceMaterialError::SpentCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialReformError::LotIdExhausted => EquipmentMaintenanceMaterialError::LotIdExhausted,
        MaterialReformError::RevisionExhausted => {
            EquipmentMaintenanceMaterialError::InventoryRevisionExhausted
        }
        MaterialReformError::StructuralLoad(error) => {
            EquipmentMaintenanceMaterialError::StructuralLoad(error)
        }
    }
}

fn map_reform_commit_error(error: MaterialReformCommitError) -> EquipmentMaintenanceCommitError {
    match error {
        MaterialReformCommitError::StaleInventoryRevision { expected, actual } => {
            EquipmentMaintenanceCommitError::StaleInventoryRevision { expected, actual }
        }
        MaterialReformCommitError::Structure(error) => {
            EquipmentMaintenanceCommitError::Structure(error)
        }
    }
}

pub(super) fn validate_maintenance_material(
    registries: &Registries,
    state: &AppState,
    record: &EquipmentRecord,
    resolution: EquipmentMaintenanceResolution,
) -> Result<ValidatedMaintenanceMaterial, EquipmentMaintenanceMaterialError> {
    match resolution.material_mode {
        EquipmentMaintenanceMaterialResolution::AggregateWearStock => {
            validate_material_reform_from_selection(
                registries,
                state,
                resolution.spent_destination,
                resolution.spent,
                resolution.material,
            )
            .map(ValidatedMaintenanceMaterial::Aggregate)
            .map_err(map_reform_error)
        }
        EquipmentMaintenanceMaterialResolution::EmbodiedComponentReplacement { component } => {
            validate_component_exchange(registries, state, record, resolution, component)
        }
    }
}
