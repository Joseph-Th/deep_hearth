//! Read-only post-egress projection used by atomic cross-owner material exchanges.

use std::collections::BTreeSet;

use crate::core::time::SimulationTick;
use crate::registry::Registries;

use super::integrity::summarize_planned_ingress_mass;
use super::{
    IngressMassSummary, MaterialIngressEntry, MaterialIngressError, ValidatedMaterialIngress,
    plan_ingress_identities, summarize_ingress_mass,
};
use crate::inventory::state::{InventoryState, StockpileId, StockpileRecord};
use crate::inventory::transactions::ValidatedMaterialEgress;

impl ValidatedMaterialIngress {
    /// Proves a projected ingress against the exact inventory state produced by its preceding egress.
    pub(crate) fn assert_matches_state_after_egress(
        &self,
        state: &InventoryState,
        egress: &ValidatedMaterialEgress,
    ) {
        assert!(self.reserved_mass.is_zero());
        egress.assert_matches_state(state);
        assert_eq!(
            self.expected_revision,
            egress.next_revision(),
            "projected material ingress must immediately follow its planned egress"
        );
        self.assert_identity_plan_matches_state(state);
        let destination_record = state.get_stockpile(self.destination).unwrap_or_else(|| {
            panic!("projected material ingress destination disappeared before commit")
        });
        let summary = summarize_planned_ingress_mass(&self.entries, self.current_tick);
        validate_ingress_capacity_after_egress(
            destination_record,
            self.destination,
            &summary,
            egress,
        )
        .unwrap_or_else(|error| panic!("projected material ingress capacity changed: {error:?}"));
    }
}

fn validate_ingress_capacity_after_egress(
    destination_record: &StockpileRecord,
    destination: StockpileId,
    summary: &IngressMassSummary,
    egress: &ValidatedMaterialEgress,
) -> Result<(), MaterialIngressError> {
    let source_is_destination = egress.source() == destination;
    let outgoing = if source_is_destination {
        egress.total_consumed()
    } else {
        crate::core::quantity::Mass::ZERO
    };
    let projection = destination_record
        .project_mass_exchange(outgoing, summary.total)
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;
    if projection.after_incoming > destination_record.capacity() {
        return Err(MaterialIngressError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: projection.committed_before_incoming,
            requested: summary.total,
        });
    }
    for (commodity, incoming) in &summary.by_commodity {
        let mut existing_after_egress = destination_record.get_mass(*commodity);
        if source_is_destination {
            for input in egress
                .inputs()
                .iter()
                .filter(|input| input.commodity() == *commodity)
            {
                existing_after_egress = existing_after_egress.checked_sub(input.mass()).ok_or(
                    MaterialIngressError::MassOverflow {
                        stockpile: destination,
                    },
                )?;
            }
        }
        existing_after_egress
            .checked_add(*incoming)
            .ok_or(MaterialIngressError::MassOverflow {
                stockpile: destination,
            })?;
    }
    Ok(())
}

/// Validates ingress against the inventory projection produced by an already validated egress.
///
/// Cross-owner exchange transactions use this to prove the destination after a preceding
/// withdrawal without cloning the complete inventory state. Fully consumed source lots are
/// excluded from identity planning; partial source lots remain merge candidates.
pub(crate) fn validate_material_ingress_after_egress(
    registries: &Registries,
    state: &InventoryState,
    egress: &ValidatedMaterialEgress,
    destination: StockpileId,
    entries: impl IntoIterator<Item = MaterialIngressEntry>,
    current_tick: SimulationTick,
) -> Result<ValidatedMaterialIngress, MaterialIngressError> {
    assert_eq!(
        state.revision(),
        egress.expected_revision(),
        "post-egress ingress projection requires the egress source revision"
    );
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
    validate_ingress_capacity_after_egress(destination_record, destination, &mass_summary, egress)?;
    let excluded_existing = if egress.source() == destination {
        egress
            .lot_slices()
            .iter()
            .filter_map(|slice| {
                state
                    .get_lot(slice.lot)
                    .and_then(|lot| (slice.mass == lot.mass()).then_some(slice.lot))
            })
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    let identity_plan = plan_ingress_identities(
        registries,
        state,
        destination_record,
        destination,
        &entries,
        current_tick,
        excluded_existing,
    )?;
    let expected_revision = egress.next_revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(MaterialIngressError::RevisionExhausted)?;

    Ok(ValidatedMaterialIngress {
        expected_revision,
        next_revision,
        destination,
        entries,
        lot_ids: identity_plan.lot_ids,
        merge_policies: identity_plan.merge_policies,
        excluded_existing: identity_plan.excluded_existing,
        next_lot_id: identity_plan.next_lot_id,
        current_tick,
        reserved_mass: crate::core::quantity::Mass::ZERO,
    })
}
