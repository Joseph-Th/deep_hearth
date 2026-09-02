//! Completion-stage custody transfer for admitted storage-enclosure dismantling work.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::labor::PlayerWork;
use crate::registry::Registries;

use super::validate_storage_dismantling_target_for_completion;
use crate::inventory::{
    InventoryState, MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId,
    StockpileStorageProfile, StorageDefinitionId, ValidatedMaterialIngress, apply_material_ingress,
    validate_reserved_material_ingress,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageEnclosureDismantlingTickError {
    MaterialLotIds,
    InventoryRevision,
}

/// Observable result of returning one enclosure body to ordinary inventory custody.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageEnclosureDismantlingOutcome {
    target: StockpileId,
    definition: StorageDefinitionId,
    recovered_lots: Vec<MaterialLotId>,
}

impl StorageEnclosureDismantlingOutcome {
    #[must_use]
    pub const fn target(&self) -> StockpileId {
        self.target
    }

    #[must_use]
    pub const fn definition(&self) -> StorageDefinitionId {
        self.definition
    }

    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

#[must_use]
pub(crate) struct StorageEnclosureDismantlingTickPlan {
    target: StockpileId,
    definition: StorageDefinitionId,
    expected_profile: StockpileStorageProfile,
    ingress: ValidatedMaterialIngress,
    next_inventory_revision: u64,
    at: SimulationTick,
}

pub(crate) fn decide_storage_enclosure_dismantling_tick(
    registries: &Registries,
    state: &AppState,
    projected_inventory: &InventoryState,
    next_tick: SimulationTick,
) -> Result<Option<StorageEnclosureDismantlingTickPlan>, StorageEnclosureDismantlingTickError> {
    let Some(PlayerWork::StorageEnclosureDismantling { work }) = state.player_work().active()
    else {
        return Ok(None);
    };
    if work.completes_at() != next_tick {
        return Ok(None);
    }
    let target = projected_inventory
        .get_stockpile(work.target())
        .unwrap_or_else(|| panic!("runtime invariant broken: dismantling target disappeared"));
    let enclosure = target
        .enclosure()
        .unwrap_or_else(|| panic!("runtime invariant broken: dismantling enclosure disappeared"));
    assert_eq!(enclosure.definition(), work.definition());
    assert_eq!(enclosure.created_at(), work.enclosure_created_at());
    assert_eq!(enclosure.embodied_mass(), work.recovered_mass());
    validate_storage_dismantling_target_for_completion(
        registries,
        projected_inventory,
        work.target(),
        next_tick,
    )
    .unwrap_or_else(|error| {
        panic!("runtime invariant broken: admitted dismantling target became invalid: {error}")
    });
    let entries = enclosure
        .embodied_material()
        .iter()
        .map(MaterialIngressEntry::from_consumed_trace)
        .collect::<Vec<_>>();
    let ingress = validate_reserved_material_ingress(
        registries,
        projected_inventory,
        work.recovery_destination(),
        entries,
        next_tick,
        work.recovered_mass(),
    )
    .map_err(|error| match error {
        MaterialIngressError::LotIdExhausted => {
            StorageEnclosureDismantlingTickError::MaterialLotIds
        }
        MaterialIngressError::RevisionExhausted => {
            StorageEnclosureDismantlingTickError::InventoryRevision
        }
        other => panic!(
            "runtime invariant broken: admitted dismantling recovery became invalid: {other:?}"
        ),
    })?;
    let next_inventory_revision = ingress
        .next_revision()
        .checked_add(1)
        .ok_or(StorageEnclosureDismantlingTickError::InventoryRevision)?;
    Ok(Some(StorageEnclosureDismantlingTickPlan {
        target: work.target(),
        definition: work.definition(),
        expected_profile: target.storage_profile(),
        ingress,
        next_inventory_revision,
        at: next_tick,
    }))
}

pub(crate) fn apply_storage_enclosure_dismantling_tick(
    state: &mut AppState,
    plan: Option<StorageEnclosureDismantlingTickPlan>,
) -> Option<StorageEnclosureDismantlingOutcome> {
    let plan = plan?;
    plan.ingress.assert_matches_state(state.inventory());
    let recovered_lots = apply_material_ingress(state.inventory_state_mut(), plan.ingress);
    state.inventory_state_mut().apply_storage_enclosure_removal(
        plan.target,
        plan.expected_profile,
        StockpileStorageProfile::unbounded_solid_only(),
        plan.definition,
        plan.at,
        plan.next_inventory_revision,
    );
    Some(StorageEnclosureDismantlingOutcome {
        target: plan.target,
        definition: plan.definition,
        recovered_lots,
    })
}
