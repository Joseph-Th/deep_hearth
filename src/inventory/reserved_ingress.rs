//! Inventory-owned allocation and commit of previously reserved production output matter.

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::MaterialLotSpec;
use crate::registry::Registries;

use super::coalescing::LotMergePolicy;
use super::state::{
    InventoryState, MaterialLotId, MaterialLotProfile, MaterialLotProvenance, MaterialLotRecord,
    MaterialStorageHistory, StockpileId, apply_insert_or_merge_new_lot, get_stockpile_mut_or_panic,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservedDepositPlanError {
    LotIdExhausted,
    RevisionExhausted,
}

/// One already-reserved production output stream awaiting authoritative inventory ingress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReservedDepositRequest {
    destination: StockpileId,
    outputs: Vec<MaterialLotSpec>,
    reserved_mass: Mass,
}

impl ReservedDepositRequest {
    pub(crate) fn new(
        destination: StockpileId,
        outputs: Vec<MaterialLotSpec>,
        reserved_mass: Mass,
    ) -> Self {
        Self {
            destination,
            outputs,
            reserved_mass,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReservedDepositPlanEntry {
    destination: StockpileId,
    outputs: Vec<MaterialLotSpec>,
    lot_ids: Vec<MaterialLotId>,
    merge_policies: Vec<LotMergePolicy>,
    reserved_mass: Mass,
}

/// Inventory-owned allocation and revision plan for one tick's reserved production outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReservedDepositPlan {
    expected_revision: u64,
    next_revision: u64,
    next_lot_id: u64,
    created_at: SimulationTick,
    entries: Vec<ReservedDepositPlanEntry>,
}

impl ReservedDepositPlan {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
}

/// Decides all material-lot identities and the single inventory revision used by one completion tick.
pub(crate) fn decide_reserved_deposits(
    registries: &Registries,
    state: &InventoryState,
    created_at: SimulationTick,
    requests: Vec<ReservedDepositRequest>,
) -> Result<ReservedDepositPlan, ReservedDepositPlanError> {
    let expected_revision = state.revision();
    if requests.is_empty() {
        return Ok(ReservedDepositPlan {
            expected_revision,
            next_revision: expected_revision,
            next_lot_id: state.next_lot_id(),
            created_at,
            entries: Vec::new(),
        });
    }

    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(ReservedDepositPlanError::RevisionExhausted)?;
    let mut next_lot_id = state.next_lot_id();
    let mut entries = Vec::with_capacity(requests.len());
    for request in requests {
        let ReservedDepositRequest {
            destination,
            outputs,
            reserved_mass,
        } = request;
        let mut lot_ids = Vec::with_capacity(outputs.len());
        let merge_policies = outputs
            .iter()
            .map(|output| LotMergePolicy::for_commodity(registries, output.commodity()))
            .collect::<Vec<_>>();
        for _ in &outputs {
            lot_ids.push(MaterialLotId::new(next_lot_id));
            next_lot_id = next_lot_id
                .checked_add(1)
                .ok_or(ReservedDepositPlanError::LotIdExhausted)?;
        }
        entries.push(ReservedDepositPlanEntry {
            destination,
            outputs,
            lot_ids,
            merge_policies,
            reserved_mass,
        });
    }

    Ok(ReservedDepositPlan {
        expected_revision,
        next_revision,
        next_lot_id,
        created_at,
        entries,
    })
}

/// Applies an inventory-owned reserved-output plan after the cross-owner transaction is prechecked.
pub(crate) fn apply_reserved_deposits(state: &mut InventoryState, plan: ReservedDepositPlan) {
    let ReservedDepositPlan {
        expected_revision,
        next_revision,
        next_lot_id,
        created_at,
        entries,
    } = plan;
    assert_eq!(
        state.revision(),
        expected_revision,
        "reserved deposit application requires its planned inventory revision"
    );
    if entries.is_empty() {
        debug_assert_eq!(next_revision, expected_revision);
        debug_assert_eq!(next_lot_id, state.next_lot_id());
        return;
    }

    for entry in entries {
        let ReservedDepositPlanEntry {
            destination,
            outputs,
            lot_ids,
            merge_policies,
            reserved_mass,
        } = entry;
        debug_assert_eq!(outputs.len(), lot_ids.len());
        debug_assert_eq!(outputs.len(), merge_policies.len());
        {
            let record = get_stockpile_mut_or_panic(state, destination);
            record.reserved_inbound = match record.reserved_inbound.checked_sub(reserved_mass) {
                Some(value) => value,
                None => panic!(
                    "reserved output mass underflow in stockpile {}",
                    destination.value()
                ),
            };
        }

        let preservation_multiplier_ppm = state
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("reserved output destination disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();

        for ((output, lot_id), merge_policy) in outputs.into_iter().zip(lot_ids).zip(merge_policies)
        {
            apply_insert_or_merge_new_lot(
                state,
                MaterialLotRecord {
                    id: lot_id,
                    stockpile: destination,
                    mass: output.mass(),
                    profile: MaterialLotProfile {
                        commodity: output.commodity(),
                        temperature: output.temperature(),
                        composition: output.composition().clone(),
                        particle_size: output.particle_size_distribution().cloned(),
                    },
                    provenance: MaterialLotProvenance {
                        earliest_created_at: created_at,
                        latest_created_at: created_at,
                    },
                    storage_history: MaterialStorageHistory::new(created_at),
                },
                merge_policy,
                created_at,
                preservation_multiplier_ppm,
            );
        }
    }
    state.apply_lot_cursor_and_revision(next_lot_id, next_revision);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{FORM_LUMP, MATERIAL_CHARCOAL, build_registries};
    use crate::core::quantity::Temperature;
    use crate::core::state::AppState;
    use crate::core::time::WorldSeed;
    use crate::inventory::add_solid_stockpile_for_test;
    use crate::material::CommodityKey;

    #[test]
    fn reserved_deposit_plan_owns_lot_ids_and_revision_advance() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_3001));
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("reserved ingress stockpile fixture failed: {error}"));
        let expected_revision = state.inventory().revision();
        let first_lot_id = state.inventory().next_lot_id();
        get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
            Mass::from_milligrams(10);
        let output = MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(500_000),
        );

        let plan = decide_reserved_deposits(
            &registries,
            state.inventory(),
            SimulationTick::new(7),
            vec![ReservedDepositRequest::new(
                destination,
                vec![output],
                Mass::from_milligrams(10),
            )],
        )
        .unwrap_or_else(|error| panic!("reserved ingress planning failed: {error:?}"));
        assert_eq!(plan.expected_revision(), expected_revision);
        assert_eq!(state.inventory().revision(), expected_revision);

        apply_reserved_deposits(state.inventory_state_mut(), plan);

        assert_eq!(state.inventory().revision(), expected_revision + 1);
        let destination_record = state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("reserved ingress destination disappeared"));
        assert_eq!(destination_record.reserved_inbound(), Mass::ZERO);
        assert_eq!(destination_record.stored_mass(), Mass::from_milligrams(10));
        let lot = state
            .inventory()
            .get_lot(MaterialLotId::new(first_lot_id))
            .unwrap_or_else(|| panic!("reserved ingress did not use inventory-owned lot cursor"));
        assert_eq!(lot.stockpile(), destination);
        assert_eq!(lot.temperature(), Temperature::from_millikelvin(500_000));
    }

    #[test]
    fn empty_reserved_deposit_plan_is_a_true_noop() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_3002));
        let before = state.clone();
        let plan = decide_reserved_deposits(
            &registries,
            state.inventory(),
            SimulationTick::new(1),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("empty reserved ingress planning failed: {error:?}"));

        apply_reserved_deposits(state.inventory_state_mut(), plan);

        assert_eq!(state, before);
    }
}
