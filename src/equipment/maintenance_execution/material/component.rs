//! Traced embodied-component replacement planning for equipment maintenance.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::equipment::maintenance_resolution::EquipmentMaintenanceResolution;
use crate::equipment::state::EquipmentRecord;
use crate::inventory::{
    MaterialEgressError, MaterialIngressEntry, MaterialIngressError, StockpileStoredMassChange,
    validate_material_egress_from_selection, validate_material_ingress_after_egress,
    validate_stockpile_stored_mass_changes, validate_unreserved_stockpile_structural_load_headroom,
};
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::ValidatedMaintenanceMaterial;
use crate::equipment::maintenance_execution::EquipmentMaintenanceMaterialError;

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
    equipment: crate::equipment::EquipmentId,
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
        | MaterialIngressError::ReservationMismatch { .. }
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            EquipmentMaintenanceMaterialError::InvalidEmbodiedComponent { equipment }
        }
    }
}

pub(super) fn validate_component_exchange(
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
        .filter(|trace| trace.profile().commodity() == component);
    let embodied = worn
        .clone()
        .try_fold(Mass::ZERO, |mass, trace| mass.checked_add(trace.mass()))
        .unwrap_or_else(|| panic!("validated maintenance trace mass overflowed"));
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
        worn.map(|trace| {
            MaterialIngressEntry::from_reformed_consumed_trace(trace, resolution.spent.form())
        }),
        state.tick(),
    )
    .map_err(|error| map_ingress_error(record.id(), error))?;
    if !state.has_material_lot_id_headroom_from(worn_ingress.next_lot_id(), 0) {
        return Err(EquipmentMaintenanceMaterialError::LotIdExhausted);
    }
    if !state.can_spend_inventory_revisions(2) {
        return Err(EquipmentMaintenanceMaterialError::InventoryRevisionExhausted);
    }

    let structural = if source == spent_destination {
        None
    } else {
        let source_after = egress.source_stored_mass_after(state.inventory());
        let spent_after =
            worn_ingress.destination_stored_mass_after_egress(state.inventory(), &egress);
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
    validate_unreserved_stockpile_structural_load_headroom(state, structural.as_ref())
        .map_err(EquipmentMaintenanceMaterialError::StructuralLoad)?;

    Ok(ValidatedMaintenanceMaterial::Component {
        component,
        replacement,
        egress,
        worn_ingress,
        structural,
    })
}
