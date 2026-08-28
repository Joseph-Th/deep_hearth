//! Same-material form-reform transactions with exact provenance, storage, and structural accounting.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialInputSpec};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::super::coalescing::LotMergePolicy;
use super::super::lot_identity::LotIdentityPlanner;
use super::super::selection::ConsumptionSelection;
use super::super::state::{
    ConsumedMaterialTrace, LotSlice, MaterialLotId, MaterialLotProfile, MaterialLotRecord,
    MaterialStorageHistory, StockpileId, StockpileRecord, apply_aggregate_withdraw,
    apply_consume_lot_slice, apply_insert_or_merge_new_lot,
};
use super::super::storage_validation::{
    CommodityReferenceError, StockpileStorageError, validate_commodity_reference,
    validate_stockpile_storage,
};
use super::super::{
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};

/// Revision-bound reforming of exact selected matter into another physical form of the same material.
///
/// The caller owns the physical reason for the form change. Inventory owns only exact withdrawal,
/// destination storage admission, conserved mass, lot identity/provenance, and structural-load updates.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialReform {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    destination: StockpileId,
    source_inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    outputs: Vec<(ConsumedMaterialTrace, MaterialStorageHistory)>,
    target: CommodityKey,
    total_mass: Mass,
    lot_ids: Vec<MaterialLotId>,
    merge_policy: LotMergePolicy,
    next_lot_id: u64,
    structural: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMaterialReform {
    pub(crate) const fn total_mass(&self) -> Mass {
        self.total_mass
    }

    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), MaterialReformCommitError> {
        let actual = state.inventory().revision();
        if actual != self.expected_revision {
            return Err(MaterialReformCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        if let Some(structural) = self.structural {
            structural
                .commit(state)
                .map_err(MaterialReformCommitError::Structure)?;
        }

        let current_tick = state.tick();
        let inventories = state.inventory_state_mut();
        let destination_preservation_multiplier_ppm = inventories
            .get_stockpile(self.destination)
            .unwrap_or_else(|| panic!("validated material reform destination disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();
        for input in &self.source_inputs {
            apply_aggregate_withdraw(inventories, self.source, input.commodity(), input.mass());
        }
        for slice in self.lot_slices {
            apply_consume_lot_slice(inventories, slice);
        }
        for ((trace, storage_history), lot_id) in self.outputs.into_iter().zip(self.lot_ids) {
            let mut profile: MaterialLotProfile = trace.profile().clone();
            profile.commodity = self.target;
            apply_insert_or_merge_new_lot(
                inventories,
                MaterialLotRecord {
                    id: lot_id,
                    stockpile: self.destination,
                    mass: trace.mass(),
                    profile,
                    provenance: trace.provenance(),
                    storage_history,
                },
                self.merge_policy,
                current_tick,
                destination_preservation_multiplier_ppm,
            );
        }
        inventories.apply_lot_cursor_and_revision(self.next_lot_id, self.next_revision);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialReformError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    UnknownTargetMaterial {
        material: MaterialId,
    },
    UnknownTargetForm {
        form: FormId,
    },
    MaterialChanged {
        source: MaterialId,
        target: MaterialId,
    },
    PhaseChanged {
        source: FormId,
        target: FormId,
    },
    TargetUnchanged {
        commodity: CommodityKey,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialReformCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MaterialReformMassPlan {
    structural: Option<ValidatedStockpileStructuralLoad>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MaterialReformIdentityPlan {
    lot_ids: Vec<MaterialLotId>,
    merge_policy: LotMergePolicy,
    next_lot_id: u64,
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

fn validate_reform_profiles(
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

fn plan_reform_mass_and_structure(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    destination: StockpileId,
    target: CommodityKey,
    total_consumed: Mass,
    expected_revision: u64,
) -> Result<MaterialReformMassPlan, MaterialReformError> {
    let inventories = state.inventory();
    let source_record = inventories
        .get_stockpile(source)
        .ok_or(MaterialReformError::UnknownSource { stockpile: source })?;
    let destination_record =
        inventories
            .get_stockpile(destination)
            .ok_or(MaterialReformError::UnknownDestination {
                stockpile: destination,
            })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(total_consumed)
        .ok_or(MaterialReformError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        })?;
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
    let committed_before_output = if source == destination {
        source_after
            .checked_add(destination_record.reserved_inbound())
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?
    } else {
        destination_record
            .stored_mass()
            .checked_add(destination_record.reserved_inbound())
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?
    };
    let after_with_reserved = committed_before_output.checked_add(total_consumed).ok_or(
        MaterialReformError::DestinationMassOverflow {
            stockpile: destination,
        },
    )?;
    if after_with_reserved > destination_record.capacity() {
        return Err(MaterialReformError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: committed_before_output,
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

fn build_reform_outputs(
    state: &AppState,
    source_record: &StockpileRecord,
    lot_slices: &[LotSlice],
    consumed_inputs: Vec<ConsumedMaterialTrace>,
) -> Vec<(ConsumedMaterialTrace, MaterialStorageHistory)> {
    let inventories = state.inventory();
    let source_preservation_multiplier_ppm = source_record
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
                .rebase(state.tick(), source_preservation_multiplier_ppm)
                .unwrap_or_else(|| {
                    panic!("valid inventory lot storage history could not be rebased for reform")
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

fn plan_reform_identities(
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

/// Validates a same-material physical-form change for one exact preselected quantity.
pub(crate) fn validate_material_reform_from_selection(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    target: CommodityKey,
    selection: ConsumptionSelection,
) -> Result<ValidatedMaterialReform, MaterialReformError> {
    let ConsumptionSelection {
        expected_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        total_consumed,
    } = selection;
    let inventories = state.inventory();
    if inventories.revision() != expected_revision {
        return Err(MaterialReformError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        });
    }
    let source_record = inventories
        .get_stockpile(source)
        .ok_or(MaterialReformError::UnknownSource { stockpile: source })?;
    let destination_record =
        inventories
            .get_stockpile(destination)
            .ok_or(MaterialReformError::UnknownDestination {
                stockpile: destination,
            })?;
    validate_reform_profiles(
        registries,
        destination_record,
        destination,
        target,
        &consumed_inputs,
    )?;
    let mass_plan = plan_reform_mass_and_structure(
        registries,
        state,
        source,
        destination,
        target,
        total_consumed,
        expected_revision,
    )?;
    let outputs = build_reform_outputs(state, source_record, &lot_slices, consumed_inputs);
    let identity_plan = plan_reform_identities(
        registries,
        state,
        destination_record,
        destination,
        target,
        &lot_slices,
        &outputs,
    )?;
    let next_revision = inventories
        .revision()
        .checked_add(1)
        .ok_or(MaterialReformError::RevisionExhausted)?;

    Ok(ValidatedMaterialReform {
        expected_revision,
        next_revision,
        source,
        destination,
        source_inputs: inputs,
        lot_slices,
        outputs,
        target,
        total_mass: total_consumed,
        lot_ids: identity_plan.lot_ids,
        merge_policy: identity_plan.merge_policy,
        next_lot_id: identity_plan.next_lot_id,
        structural: mass_plan.structural,
    })
}
