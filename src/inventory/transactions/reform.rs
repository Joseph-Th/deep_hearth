//! Same-material form-reform transactions with exact provenance, storage, and structural accounting.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::registry::Registries;

use super::super::coalescing::LotMergePolicy;
use super::super::selection::ConsumptionSelection;
use super::super::state::{
    ConsumedMaterialTrace, LotSlice, MaterialLotId, MaterialLotProfile, MaterialLotRecord,
    MaterialStorageHistory, StockpileId, apply_aggregate_withdraw, apply_consume_lot_slice,
    apply_insert_or_merge_new_lot, checked_consumed_material_mass,
};
use super::super::{
    ValidatedStockpileStructuralLoad, validate_unreserved_stockpile_structural_load_headroom,
};

mod errors;
mod integrity;
mod planning;

pub(crate) use errors::{MaterialReformCommitError, MaterialReformError};
use planning::{
    build_reform_outputs, plan_reform_identities, plan_reform_mass_and_structure,
    validate_reform_profiles,
};

/// Revision-bound reforming of exact selected matter into another physical form of the same material.
///
/// The caller owns the physical reason for the form change. Inventory owns only exact withdrawal,
/// destination storage admission, conserved mass, lot identity/provenance, and structural-load updates.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialReform {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    destination: StockpileId,
    source_inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    outputs: Vec<(ConsumedMaterialTrace, MaterialStorageHistory)>,
    target: CommodityKey,
    lot_ids: Vec<MaterialLotId>,
    merge_policy: LotMergePolicy,
    next_lot_id: u64,
    structural: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMaterialReform {
    pub(crate) fn total_mass(&self) -> Mass {
        self.outputs.iter().fold(Mass::ZERO, |total, (trace, _)| {
            total
                .checked_add(trace.mass())
                .unwrap_or_else(|| panic!("validated material reform mass overflowed"))
        })
    }

    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), MaterialReformCommitError> {
        let actual = state.inventory().revision();
        if actual != self.expected_revision {
            return Err(MaterialReformCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        self.assert_matches_state(state);
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
    } = selection;
    let total_consumed = checked_consumed_material_mass(&consumed_inputs)
        .unwrap_or_else(|| panic!("validated consumption selection mass overflowed before reform"));
    let inventories = state.inventory();
    if inventories.revision() != expected_revision {
        return Err(MaterialReformError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        });
    }
    let source_record = inventories
        .get_stockpile(source)
        .unwrap_or_else(|| panic!("validated material reform source disappeared"));
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
    )?;
    validate_unreserved_stockpile_structural_load_headroom(state, mass_plan.structural.as_ref())
        .map_err(MaterialReformError::StructuralLoad)?;
    let outputs = build_reform_outputs(
        state,
        source_record,
        destination_record,
        &lot_slices,
        consumed_inputs,
    );
    let identity_plan = plan_reform_identities(
        registries,
        state,
        destination_record,
        destination,
        target,
        &lot_slices,
        &outputs,
    )?;
    if !state.has_material_lot_id_headroom_from(identity_plan.next_lot_id, 0) {
        return Err(MaterialReformError::LotIdExhausted);
    }
    if !state.can_spend_inventory_revisions(1) {
        return Err(MaterialReformError::RevisionExhausted);
    }
    let next_revision = inventories
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("inventory headroom check includes reform revision"));

    Ok(ValidatedMaterialReform {
        expected_revision,
        next_revision,
        source,
        destination,
        source_inputs: inputs,
        lot_slices,
        outputs,
        target,
        lot_ids: identity_plan.lot_ids,
        merge_policy: identity_plan.merge_policy,
        next_lot_id: identity_plan.next_lot_id,
        structural: mass_plan.structural,
    })
}
