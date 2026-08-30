//! Exact material exchange owned by equipment maintenance.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, MaterialEgressError, MaterialIngressEntry, MaterialIngressError,
    MaterialReformCommitError, MaterialReformError, StockpileStoredMassChange,
    ValidatedMaterialEgress, ValidatedMaterialIngress, ValidatedMaterialReform,
    ValidatedStockpileStructuralLoad, apply_material_egress, apply_material_ingress,
    checked_consumed_material_mass, validate_material_egress_from_selection,
    validate_material_ingress_after_egress, validate_material_reform_from_selection,
    validate_stockpile_stored_mass_changes,
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
        condition_after: crate::maintenance::Condition,
        expected_equipment_revision: u64,
        next_equipment_revision: u64,
    ) -> Result<(), EquipmentMaintenanceCommitError> {
        match self {
            Self::Aggregate(material) => {
                state.equipment().assert_condition_change_available(
                    equipment,
                    condition_before,
                    next_equipment_revision,
                );
                material.commit(state).map_err(map_reform_commit_error)?;
                state.equipment_state_mut().apply_condition_change(
                    equipment,
                    condition_before,
                    condition_after,
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
                    condition_after,
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

fn trace_mass(traces: &[ConsumedMaterialTrace]) -> Mass {
    checked_consumed_material_mass(traces)
        .unwrap_or_else(|| panic!("validated maintenance trace mass overflowed"))
}

fn map_egress_error(error: MaterialEgressError) -> EquipmentMaintenanceMaterialError {
    match error {
        MaterialEgressError::StaleSelection { expected, actual } => {
            EquipmentMaintenanceMaterialError::StaleSelection { expected, actual }
        }
        MaterialEgressError::RevisionExhausted => {
            EquipmentMaintenanceMaterialError::InventoryRevisionExhausted
        }
    }
}

fn map_ingress_error(
    equipment: EquipmentId,
    error: MaterialIngressError,
) -> EquipmentMaintenanceMaterialError {
    match error {
        MaterialIngressError::UnknownStockpile { stockpile } => {
            EquipmentMaintenanceMaterialError::UnknownSpentDestination { stockpile }
        }
        MaterialIngressError::UnknownMaterial { material } => {
            EquipmentMaintenanceMaterialError::UnknownSpentMaterial { material }
        }
        MaterialIngressError::UnknownForm { form } => {
            EquipmentMaintenanceMaterialError::UnknownSpentForm { form }
        }
        MaterialIngressError::Storage(error) => {
            EquipmentMaintenanceMaterialError::SpentStorage(error)
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            EquipmentMaintenanceMaterialError::SpentMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
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
        MaterialIngressError::LotIdExhausted => EquipmentMaintenanceMaterialError::LotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            EquipmentMaintenanceMaterialError::InventoryRevisionExhausted
        }
        MaterialIngressError::Empty
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::InvalidProvenance
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            EquipmentMaintenanceMaterialError::InvalidEmbodiedComponent { equipment }
        }
    }
}

fn map_reform_error(error: MaterialReformError) -> EquipmentMaintenanceMaterialError {
    match error {
        MaterialReformError::StaleSelection { expected, actual } => {
            EquipmentMaintenanceMaterialError::StaleSelection { expected, actual }
        }
        MaterialReformError::UnknownSource { stockpile } => {
            EquipmentMaintenanceMaterialError::UnknownSource { stockpile }
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

fn validate_component_exchange(
    registries: &Registries,
    state: &AppState,
    record: &EquipmentRecord,
    resolution: EquipmentMaintenanceResolution,
    component: CommodityKey,
) -> Result<ValidatedMaintenanceMaterial, EquipmentMaintenanceMaterialError> {
    let replacement = resolution.material.consumed_inputs().to_vec();
    let required = resolution.material.total_consumed();
    let worn = record
        .embodied_material()
        .iter()
        .filter(|trace| trace.profile().commodity() == component)
        .cloned()
        .collect::<Vec<_>>();
    let embodied = trace_mass(&worn);
    if embodied != required {
        return Err(
            EquipmentMaintenanceMaterialError::EmbodiedComponentMismatch {
                equipment: record.id(),
                component,
                embodied,
                required,
            },
        );
    }

    let source = resolution.material.source();
    let spent_destination = resolution.spent_destination;
    let egress = validate_material_egress_from_selection(state.inventory(), resolution.material)
        .map_err(map_egress_error)?;

    let worn_ingress = validate_material_ingress_after_egress(
        registries,
        state.inventory(),
        &egress,
        spent_destination,
        worn.iter().map(|trace| {
            MaterialIngressEntry::from_reformed_consumed_trace(trace, resolution.spent.form())
        }),
        state.tick(),
    )
    .map_err(|error| map_ingress_error(record.id(), error))?;

    let structural = if source == spent_destination {
        None
    } else {
        let source_record = state
            .inventory()
            .get_stockpile(source)
            .ok_or(EquipmentMaintenanceMaterialError::UnknownSource { stockpile: source })?;
        let spent_record = state.inventory().get_stockpile(spent_destination).ok_or(
            EquipmentMaintenanceMaterialError::UnknownSpentDestination {
                stockpile: spent_destination,
            },
        )?;
        let source_after = source_record
            .stored_mass()
            .checked_sub(required)
            .ok_or(EquipmentMaintenanceMaterialError::SpentMassOverflow { stockpile: source })?;
        let spent_after = spent_record.stored_mass().checked_add(required).ok_or(
            EquipmentMaintenanceMaterialError::SpentMassOverflow {
                stockpile: spent_destination,
            },
        )?;
        validate_stockpile_stored_mass_changes(
            registries,
            state,
            [
                StockpileStoredMassChange::new(source, source_after),
                StockpileStoredMassChange::new(spent_destination, spent_after),
            ],
        )
        .map_err(EquipmentMaintenanceMaterialError::StructuralLoad)?
    };

    Ok(ValidatedMaintenanceMaterial::Component {
        component,
        replacement,
        egress,
        worn_ingress,
        structural,
    })
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
