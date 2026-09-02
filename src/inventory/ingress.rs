//! Canonical admission of already-owned material into inventory.
//!
//! Source systems normalize both single-lot and multi-lot transfers into `MaterialIngressEntry`
//! values. Validation allocates destination lot identities and binds one inventory revision; apply
//! performs the corresponding owner mutation exactly once. No alternate single-lot ingress path is
//! retained.

use std::collections::BTreeSet;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
#[cfg(any(test, feature = "test-gameplay"))]
use crate::material::MaterialLotSpec;
use crate::material::{CommodityKey, FormId};
use crate::registry::Registries;

use super::coalescing::LotMergePolicy;
use super::state::{
    ConsumedMaterialTrace, InventoryState, MaterialLotId, MaterialLotProfile,
    MaterialLotProvenance, MaterialLotRecord, MaterialStorageHistory, StockpileId,
    apply_insert_or_merge_new_lot, get_stockpile_mut_or_panic,
};

mod errors;
mod identity;
mod integrity;
mod projection;
mod validation;

pub(crate) use errors::MaterialIngressError;
use identity::{plan_ingress_identities, replay_ingress_identity_plan};
pub(crate) use projection::validate_material_ingress_after_egress;
use validation::{
    IngressMassSummary, summarize_ingress_mass, validate_ingress_capacity,
    validate_ingress_capacity_with_reserved_credit,
};

/// One source-owned material parcel prepared for canonical inventory admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MaterialIngressEntry {
    mass: Mass,
    profile: MaterialLotProfile,
    provenance: MaterialLotProvenance,
}

impl MaterialIngressEntry {
    /// Converts a newly created lot specification into an ingress parcel with exact provenance.
    #[cfg(any(test, feature = "test-gameplay"))]
    #[must_use]
    pub(crate) fn from_lot_spec(
        specification: MaterialLotSpec,
        created_at: SimulationTick,
    ) -> Self {
        Self {
            mass: specification.mass(),
            profile: MaterialLotProfile {
                commodity: specification.commodity(),
                temperature: specification.temperature(),
                composition: specification.composition().clone(),
                particle_size: specification.particle_size_distribution().cloned(),
            },
            provenance: MaterialLotProvenance {
                earliest_created_at: created_at,
                latest_created_at: created_at,
            },
        }
    }

    /// Preserves the complete material profile and lot provenance of matter transferred from
    /// another owner. Inventory storage exposure starts when custody returns to a stockpile.
    #[must_use]
    pub(crate) fn from_consumed_trace(trace: &ConsumedMaterialTrace) -> Self {
        Self {
            mass: trace.mass(),
            profile: trace.profile().clone(),
            provenance: trace.provenance(),
        }
    }

    /// Preserves matter, thermal state, composition, and provenance while an owning subsystem
    /// physically degrades a consolidated parcel into another form of the same material.
    #[must_use]
    pub(crate) fn from_reformed_consumed_trace(
        trace: &ConsumedMaterialTrace,
        target_form: FormId,
    ) -> Self {
        let mut profile = trace.profile().clone();
        profile.commodity = CommodityKey::new(profile.commodity().material(), target_form);
        Self {
            mass: trace.mass(),
            profile,
            provenance: trace.provenance(),
        }
    }
}

/// Consumed proof that a complete source-owned parcel set can enter one stockpile atomically.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialIngress {
    expected_revision: u64,
    next_revision: u64,
    destination: StockpileId,
    entries: Vec<MaterialIngressEntry>,
    lot_ids: Vec<MaterialLotId>,
    merge_policies: Vec<LotMergePolicy>,
    excluded_existing: BTreeSet<MaterialLotId>,
    next_lot_id: u64,
    current_tick: SimulationTick,
    reserved_mass: Mass,
}

impl ValidatedMaterialIngress {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(crate) const fn next_revision(&self) -> u64 {
        self.next_revision
    }
}

/// Validates all material parcels entering one stockpile under one inventory revision.
pub(crate) fn validate_material_ingress(
    registries: &Registries,
    state: &InventoryState,
    destination: StockpileId,
    entries: impl IntoIterator<Item = MaterialIngressEntry>,
    current_tick: SimulationTick,
) -> Result<ValidatedMaterialIngress, MaterialIngressError> {
    let entries = entries.into_iter().collect::<Vec<_>>();
    if entries.is_empty() {
        return Err(MaterialIngressError::Empty);
    }
    let Some(destination_record) = state.get_stockpile(destination) else {
        return Err(MaterialIngressError::UnknownStockpile {
            stockpile: destination,
        });
    };
    let mass_summary = summarize_ingress_mass(
        registries,
        destination_record,
        destination,
        &entries,
        current_tick,
    )?;
    validate_ingress_capacity(destination_record, destination, &mass_summary)?;
    let identity_plan = plan_ingress_identities(
        registries,
        state,
        destination_record,
        destination,
        &entries,
        current_tick,
        BTreeSet::new(),
    )?;
    let next_revision = state
        .revision()
        .checked_add(1)
        .ok_or(MaterialIngressError::RevisionExhausted)?;

    Ok(ValidatedMaterialIngress {
        expected_revision: state.revision(),
        next_revision,
        destination,
        entries,
        lot_ids: identity_plan.lot_ids,
        merge_policies: identity_plan.merge_policies,
        excluded_existing: identity_plan.excluded_existing,
        next_lot_id: identity_plan.next_lot_id,
        current_tick,
        reserved_mass: Mass::ZERO,
    })
}

/// Validates exact source-owned parcels returning to inventory against capacity already reserved
/// for this same matter. The reserved mass is consumed atomically by the resulting ingress.
pub(crate) fn validate_reserved_material_ingress(
    registries: &Registries,
    state: &InventoryState,
    destination: StockpileId,
    entries: impl IntoIterator<Item = MaterialIngressEntry>,
    current_tick: SimulationTick,
    reserved_mass: Mass,
) -> Result<ValidatedMaterialIngress, MaterialIngressError> {
    let entries = entries.into_iter().collect::<Vec<_>>();
    if entries.is_empty() {
        return Err(MaterialIngressError::Empty);
    }
    let Some(destination_record) = state.get_stockpile(destination) else {
        return Err(MaterialIngressError::UnknownStockpile {
            stockpile: destination,
        });
    };
    let mass_summary = summarize_ingress_mass(
        registries,
        destination_record,
        destination,
        &entries,
        current_tick,
    )?;
    if mass_summary.total != reserved_mass {
        return Err(MaterialIngressError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: destination_record.stored_mass(),
            requested: mass_summary.total,
        });
    }
    validate_ingress_capacity_with_reserved_credit(
        destination_record,
        destination,
        &mass_summary,
        reserved_mass,
    )?;
    let identity_plan = plan_ingress_identities(
        registries,
        state,
        destination_record,
        destination,
        &entries,
        current_tick,
        BTreeSet::new(),
    )?;
    let next_revision = state
        .revision()
        .checked_add(1)
        .ok_or(MaterialIngressError::RevisionExhausted)?;
    Ok(ValidatedMaterialIngress {
        expected_revision: state.revision(),
        next_revision,
        destination,
        entries,
        lot_ids: identity_plan.lot_ids,
        merge_policies: identity_plan.merge_policies,
        excluded_existing: identity_plan.excluded_existing,
        next_lot_id: identity_plan.next_lot_id,
        current_tick,
        reserved_mass,
    })
}

/// Applies a validated parcel set after its cross-owner transaction rechecks inventory revision.
pub(crate) fn apply_material_ingress(
    state: &mut InventoryState,
    ingress: ValidatedMaterialIngress,
) -> Vec<MaterialLotId> {
    ingress.assert_matches_state(state);
    let ValidatedMaterialIngress {
        expected_revision,
        next_revision,
        destination,
        entries,
        lot_ids,
        merge_policies,
        excluded_existing: _,
        next_lot_id,
        current_tick,
        reserved_mass,
    } = ingress;
    assert_eq!(
        state.revision(),
        expected_revision,
        "material ingress commit requires its validated inventory revision"
    );

    let preservation_multiplier_ppm = state
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("validated material ingress destination disappeared"))
        .storage_profile()
        .preservation_multiplier_ppm();
    if !reserved_mass.is_zero() {
        let destination_record = get_stockpile_mut_or_panic(state, destination);
        destination_record.reserved_inbound = destination_record
            .reserved_inbound
            .checked_sub(reserved_mass)
            .unwrap_or_else(|| panic!("validated reserved ingress reservation underflowed"));
    }

    let mut resulting_lots = Vec::with_capacity(entries.len());
    for ((entry, lot_id), merge_policy) in entries.into_iter().zip(lot_ids).zip(merge_policies) {
        let resulting = apply_insert_or_merge_new_lot(
            state,
            MaterialLotRecord {
                id: lot_id,
                stockpile: destination,
                mass: entry.mass,
                profile: entry.profile,
                provenance: entry.provenance,
                storage_history: MaterialStorageHistory::new(current_tick),
            },
            merge_policy,
            current_tick,
            preservation_multiplier_ppm,
        );
        resulting_lots.push(resulting);
    }
    state.apply_lot_cursor_and_revision(next_lot_id, next_revision);
    resulting_lots
}

#[cfg(test)]
#[path = "ingress_tests.rs"]
mod tests;
