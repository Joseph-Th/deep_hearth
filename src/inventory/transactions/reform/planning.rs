//! Read-only profile, mass, storage-history, and lot-identity planning for material reform.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::coalescing::LotMergePolicy;
use crate::inventory::lot_identity::LotIdentityPlanner;
use crate::inventory::state::{
    ConsumedMaterialTrace, LotSlice, MaterialLotId, MaterialLotProfile, MaterialStorageHistory,
    StockpileId, StockpileRecord,
};
use crate::inventory::storage_validation::{
    CommodityReferenceError, StockpileStorageError, validate_commodity_reference,
    validate_stockpile_storage,
};
use crate::inventory::{
    StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::errors::MaterialReformError;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct MaterialReformMassPlan {
    pub(super) structural: Option<ValidatedStockpileStructuralLoad>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MaterialReformIdentityPlan {
    pub(super) lot_ids: Vec<MaterialLotId>,
    pub(super) merge_policy: LotMergePolicy,
    pub(super) next_lot_id: u64,
}

fn map_target_reference_error(error: CommodityReferenceError) -> MaterialReformError {
    match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            MaterialReformError::UnknownTargetMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => {
            MaterialReformError::UnknownTargetForm { form }
        }
        CommodityReferenceError::UnsupportedCommodity { commodity } => {
            MaterialReformError::DestinationStorage(StockpileStorageError::UnsupportedCommodity {
                commodity,
            })
        }
    }
}

pub(super) fn validate_reform_profiles(
    registries: &Registries,
    destination_record: &StockpileRecord,
    destination: StockpileId,
    target: CommodityKey,
    consumed_inputs: &[ConsumedMaterialTrace],
) -> Result<(), MaterialReformError> {
    validate_commodity_reference(registries, target).map_err(map_target_reference_error)?;
    let target_form = registries
        .materials()
        .get_form(target.form())
        .unwrap_or_else(|| unreachable!("validated material reform target has its form"));
    if consumed_inputs
        .iter()
        .all(|trace| trace.profile().commodity() == target)
    {
        return Err(MaterialReformError::TargetUnchanged { commodity: target });
    }
    for trace in consumed_inputs {
        let source_material = trace.profile().commodity().material();
        if source_material != target.material() {
            return Err(MaterialReformError::MaterialChanged {
                source: source_material,
                target: target.material(),
            });
        }
        let source_form_id = trace.profile().commodity().form();
        let source_form = registries
            .materials()
            .get_form(source_form_id)
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: material reform source references missing form {}",
                    source_form_id.value()
                )
            });
        if source_form.phase() != target_form.phase() {
            return Err(MaterialReformError::PhaseChanged {
                source: source_form_id,
                target: target.form(),
            });
        }
        validate_stockpile_storage(
            registries,
            destination_record,
            destination,
            target,
            trace.profile().composition(),
            trace.profile().temperature(),
            trace.profile().particle_size_distribution(),
        )
        .map_err(MaterialReformError::DestinationStorage)?;
    }
    Ok(())
}

pub(super) fn plan_reform_mass_and_structure(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    destination: StockpileId,
    target: CommodityKey,
    total_consumed: Mass,
) -> Result<MaterialReformMassPlan, MaterialReformError> {
    let inventories = state.inventory();
    let source_record = inventories
        .get_stockpile(source)
        .unwrap_or_else(|| panic!("validated material reform source disappeared"));
    let destination_record =
        inventories
            .get_stockpile(destination)
            .ok_or(MaterialReformError::UnknownDestination {
                stockpile: destination,
            })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(total_consumed)
        .unwrap_or_else(|| panic!("validated material reform exceeds source stored mass"));
    let destination_after = if source == destination {
        source_record.stored_mass()
    } else {
        destination_record
            .stored_mass()
            .checked_add(total_consumed)
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?
    };
    let outgoing = if source == destination {
        total_consumed
    } else {
        Mass::ZERO
    };
    let projection = destination_record
        .project_mass_exchange(outgoing, total_consumed)
        .ok_or(MaterialReformError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    if projection.after_incoming > destination_record.capacity() {
        return Err(MaterialReformError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: projection.committed_before_incoming,
            requested: total_consumed,
        });
    }
    if source != destination {
        destination_record
            .get_mass(target)
            .checked_add(total_consumed)
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?;
    }
    let structural = if source == destination {
        None
    } else {
        validate_stockpile_stored_mass_changes(
            registries,
            state,
            [
                StockpileStoredMassChange::new(source, source_after),
                StockpileStoredMassChange::new(destination, destination_after),
            ],
        )
        .map_err(MaterialReformError::StructuralLoad)?
    };
    Ok(MaterialReformMassPlan { structural })
}

pub(super) fn build_reform_outputs(
    state: &AppState,
    source_record: &StockpileRecord,
    destination_record: &StockpileRecord,
    lot_slices: &[LotSlice],
    consumed_inputs: Vec<ConsumedMaterialTrace>,
) -> Vec<(ConsumedMaterialTrace, MaterialStorageHistory)> {
    let inventories = state.inventory();
    let source_preservation_multiplier_ppm = source_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let destination_preservation_multiplier_ppm = destination_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let output_storage_histories = lot_slices
        .iter()
        .map(|slice| {
            inventories
                .get_lot(slice.lot)
                .unwrap_or_else(|| {
                    panic!(
                        "validated material reform references missing lot {}",
                        slice.lot.value()
                    )
                })
                .storage_history()
                .transition_preservation(
                    state.tick(),
                    source_preservation_multiplier_ppm,
                    destination_preservation_multiplier_ppm,
                )
                .unwrap_or_else(|| {
                    panic!("valid inventory lot storage history could not transition for reform")
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        output_storage_histories.len(),
        consumed_inputs.len(),
        "consumption selection trace count must match selected lot slices"
    );
    consumed_inputs
        .into_iter()
        .zip(output_storage_histories)
        .collect()
}

pub(super) fn plan_reform_identities(
    registries: &Registries,
    state: &AppState,
    destination_record: &StockpileRecord,
    destination: StockpileId,
    target: CommodityKey,
    lot_slices: &[LotSlice],
    outputs: &[(ConsumedMaterialTrace, MaterialStorageHistory)],
) -> Result<MaterialReformIdentityPlan, MaterialReformError> {
    let inventories = state.inventory();
    let excluded_existing = lot_slices.iter().filter_map(|slice| {
        inventories
            .get_lot(slice.lot)
            .and_then(|lot| (slice.mass == lot.mass()).then_some(slice.lot))
    });
    let merge_policy = LotMergePolicy::for_commodity(registries, target);
    let destination_preservation_multiplier_ppm = destination_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let mut identity_planner = LotIdentityPlanner::new(inventories, excluded_existing);
    let mut lot_ids = Vec::with_capacity(outputs.len());
    for (trace, storage_history) in outputs {
        let mut profile: MaterialLotProfile = trace.profile().clone();
        profile.commodity = target;
        lot_ids.push(
            identity_planner
                .plan(
                    destination,
                    &profile,
                    *storage_history,
                    state.tick(),
                    destination_preservation_multiplier_ppm,
                    merge_policy,
                )
                .ok_or(MaterialReformError::LotIdExhausted)?,
        );
    }
    Ok(MaterialReformIdentityPlan {
        lot_ids,
        merge_policy,
        next_lot_id: identity_planner.next_lot_id(),
    })
}
