//! Construction-material selection, preservation transition, and exact egress.

use crate::core::state::AppState;
use crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE;
use crate::registry::Registries;

use super::super::storage_validation::{
    validate_residual_only_storage, validate_stockpile_storage_profile,
};
use super::super::{
    ConsumedMaterialTrace, ConsumptionSelection, ConsumptionSelectionError, MaterialEgressError,
    StockpileId, StockpileRecord, StockpileStoredMassChange, StorageDefinition,
    TemperatureLimitedSelectionError, ValidatedMaterialEgress, ValidatedStockpileStructuralLoad,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
    validate_temperature_limited_consumption_selection,
};
use super::StorageEnclosureConstructionError;

pub(super) struct EnclosureMaterialPlan {
    pub(super) embodied_material: Vec<ConsumedMaterialTrace>,
    pub(super) egress: ValidatedMaterialEgress,
    pub(super) structural_load: Option<ValidatedStockpileStructuralLoad>,
}

/// Both placement and in-place construction enforce the same body limit,
/// distinct from the future enclosure's admissible contents temperature.
pub(super) fn select_enclosure_material(
    state: &AppState,
    definition: &StorageDefinition,
    source: StockpileId,
) -> Result<ConsumptionSelection, StorageEnclosureConstructionError> {
    let inputs = definition.assembly_profile().inputs();
    match validate_temperature_limited_consumption_selection(
        state.inventory(),
        source,
        inputs,
        MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
    ) {
        Ok(selected) => Ok(selected),
        Err(TemperatureLimitedSelectionError::TooHot {
            commodity,
            temperature,
            maximum,
        }) => Err(
            StorageEnclosureConstructionError::ConstructionMaterialTooHot {
                commodity,
                temperature,
                maximum,
            },
        ),
        Err(TemperatureLimitedSelectionError::Selection(error)) => Err(map_selection_error(error)),
    }
}

fn map_selection_error(error: ConsumptionSelectionError) -> StorageEnclosureConstructionError {
    match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            StorageEnclosureConstructionError::UnknownSource { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => StorageEnclosureConstructionError::InsufficientMaterial {
            stockpile,
            commodity,
            available,
            required: requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            StorageEnclosureConstructionError::SourceMassOverflow { stockpile }
        }
    }
}

pub(super) fn validate_enclosure_contents(
    registries: &Registries,
    state: &AppState,
    target_record: &StockpileRecord,
    selection: &ConsumptionSelection,
    definition: &StorageDefinition,
) -> Result<(), StorageEnclosureConstructionError> {
    let target = target_record.id();
    let source = selection.source();
    let next_profile = definition.storage_profile();
    let source_preservation = target_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let destination_preservation = next_profile.preservation_multiplier_ppm();
    for lot in state.inventory().lot_ids(target) {
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| unreachable!("stockpile lot index references a live lot"));
        if source == target && selection.selected_mass_for_lot(lot) == record.mass() {
            continue;
        }
        if definition.residual_only() {
            validate_residual_only_storage(registries, target, record.commodity()).map_err(
                |error| StorageEnclosureConstructionError::TargetContentsIncompatible {
                    lot,
                    error,
                },
            )?;
        }
        validate_stockpile_storage_profile(
            registries,
            next_profile,
            target,
            record.commodity(),
            record.composition(),
            record.temperature(),
            record.particle_size_distribution(),
        )
        .map_err(|error| {
            StorageEnclosureConstructionError::TargetContentsIncompatible { lot, error }
        })?;
        assert!(
            record
                .storage_history()
                .transition_preservation(
                    state.tick(),
                    source_preservation,
                    destination_preservation
                )
                .is_some(),
            "runtime invariant broken: physically reachable storage history must checkpoint during enclosure construction"
        );
    }
    Ok(())
}

pub(super) fn plan_enclosure_materials(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    selection: ConsumptionSelection,
) -> Result<EnclosureMaterialPlan, StorageEnclosureConstructionError> {
    let embodied_material = selection.consumed_inputs().to_vec();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                MaterialEgressError::StaleSelection { .. } => {
                    unreachable!(
                        "storage construction selection was derived from the current revision"
                    )
                }
                MaterialEgressError::RevisionExhausted => {
                    StorageEnclosureConstructionError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_after = egress.source_stored_mass_after(state.inventory());
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(StorageEnclosureConstructionError::StructuralLoad)?;
    Ok(EnclosureMaterialPlan {
        embodied_material,
        egress,
        structural_load,
    })
}
