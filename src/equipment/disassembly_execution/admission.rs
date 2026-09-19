//! Read-only equipment-disassembly recovery and admission planning.

use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, MaterialIngressEntry, MaterialIngressError, StockpileId,
    StockpileStoredMassChange, validate_material_ingress, validate_stockpile_stored_mass_changes,
    validate_unreserved_stockpile_structural_load_headroom,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, FormId};
use crate::registry::Registries;

use super::super::{EquipmentId, EquipmentOccupancy, EquipmentRecord, equipment_occupancy};
use super::{EquipmentDisassemblyError, ValidatedEquipmentDisassembly};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EquipmentDisassemblyRecovery {
    Exact,
    WornComponent {
        component: CommodityKey,
        spent_form: FormId,
    },
}

impl EquipmentDisassemblyRecovery {
    fn ingress_entry(self, trace: &ConsumedMaterialTrace) -> MaterialIngressEntry {
        match self {
            Self::Exact => MaterialIngressEntry::from_consumed_trace(trace),
            Self::WornComponent {
                component,
                spent_form,
            } if trace.profile().commodity() == component => {
                MaterialIngressEntry::from_reformed_consumed_trace(trace, spent_form)
            }
            Self::WornComponent { .. } => MaterialIngressEntry::from_consumed_trace(trace),
        }
    }
}

fn resolve_disassembly_recovery(
    registries: &Registries,
    record: &EquipmentRecord,
) -> Result<EquipmentDisassemblyRecovery, EquipmentDisassemblyError> {
    if record.condition() == Condition::PRISTINE {
        return Ok(EquipmentDisassemblyRecovery::Exact);
    }
    let equipment = record.id();
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .ok_or(EquipmentDisassemblyError::InvalidEmbodiedMatter { equipment })?;
    if let Some(maintenance) = definition
        .maintenance_profile()
        .filter(|profile| profile.is_component_replacement())
    {
        return Ok(EquipmentDisassemblyRecovery::WornComponent {
            component: maintenance.replacement(),
            spent_form: maintenance.spent().form(),
        });
    }
    Err(
        EquipmentDisassemblyError::WornComponentRecoveryUnavailable {
            equipment,
            condition: record.condition(),
        },
    )
}

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
        | MaterialIngressError::ReservationMismatch { .. }
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            EquipmentDisassemblyError::InvalidEmbodiedMatter { equipment }
        }
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
    let recovery = resolve_disassembly_recovery(registries, record)?;
    if let Some(element) = record.supported_by() {
        return Err(EquipmentDisassemblyError::EquipmentMounted { equipment, element });
    }
    if let Some(error) = validation_occupancy_error(state, equipment) {
        return Err(error);
    }

    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        destination,
        record
            .embodied_material()
            .iter()
            .map(|trace| recovery.ingress_entry(trace)),
        state.tick(),
    )
    .map_err(|error| map_ingress_error(equipment, error))?;
    if !state.has_material_lot_id_headroom_from(ingress.next_lot_id(), 0) {
        return Err(EquipmentDisassemblyError::LotIdExhausted);
    }
    let destination_after = ingress.destination_stored_mass_after(state.inventory());
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(
            destination,
            destination_after,
        )],
    )
    .map_err(EquipmentDisassemblyError::StoredMatterLoad)?;
    validate_unreserved_stockpile_structural_load_headroom(state, structural_load.as_ref())
        .map_err(EquipmentDisassemblyError::StoredMatterLoad)?;
    let expected_equipment_revision = state.equipment().revision();
    if !state.can_spend_inventory_revisions(1) {
        return Err(EquipmentDisassemblyError::InventoryRevisionExhausted);
    }
    if !state.can_spend_equipment_revisions(1) {
        return Err(EquipmentDisassemblyError::EquipmentRevisionExhausted);
    }
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("equipment headroom check includes disassembly revision"));

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
