//! Material-backed construction of preservation enclosures around existing stockpiles.

use crate::core::state::AppState;
use crate::logistics::{
    ValidatedGroundStockpileAllocation, validate_allocate_player_ground_stockpile,
    validate_player_stockpile_access,
};
use crate::registry::Registries;

use super::storage_validation::validate_stockpile_storage_profile;
use super::{
    ConsumedMaterialTrace, ConsumptionSelection, ConsumptionSelectionError, MaterialEgressError,
    StockpileEnclosureRecord, StockpileId, StockpileRecord, StockpileStorageProfile,
    StockpileStoredMassChange, StorageDefinition, StorageDefinitionId, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
    validate_unreserved_stockpile_structural_load_headroom,
};

mod errors;

pub use errors::{
    PlayerStoragePlacementCommitError, PlayerStoragePlacementError, StorageEnclosureCommitError,
    StorageEnclosureConstructionError,
};

/// Revision-bound proof that exact construction matter can become one stockpile enclosure.
#[must_use]
pub struct ValidatedStorageEnclosureConstruction {
    target: StockpileId,
    expected_inventory_revision: u64,
    expected_logistics_revision: u64,
    next_inventory_revision: u64,
    expected_profile: StockpileStorageProfile,
    next_profile: StockpileStorageProfile,
    enclosure: StockpileEnclosureRecord,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

/// Revision-bound ordinary player placement of one authored storage enclosure from carried matter.
///
/// The token composes the implementation-only empty target allocation with exact enclosure-body
/// egress so the player action creates a finished local store rather than exposing an intermediate
/// ambient stockpile.
#[must_use]
pub struct ValidatedPlayerStoragePlacement {
    allocation: ValidatedGroundStockpileAllocation,
    next_inventory_revision: u64,
    next_profile: StockpileStorageProfile,
    enclosure: StockpileEnclosureRecord,
    egress: ValidatedMaterialEgress,
}

struct EnclosureMaterialPlan {
    embodied_material: Vec<ConsumedMaterialTrace>,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedPlayerStoragePlacement {
    #[must_use]
    pub const fn stockpile(&self) -> StockpileId {
        self.allocation.stockpile()
    }

    /// Creates the local target, transfers its carried construction matter, and applies the
    /// authored storage profile. Typed failure can only occur before target allocation mutates
    /// state; the remaining steps are consequences of the same prevalidated snapshot.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StockpileId, PlayerStoragePlacementCommitError> {
        let target = self
            .allocation
            .commit(state)
            .map_err(PlayerStoragePlacementCommitError::Allocation)?;
        self.egress.assert_matches_state(state.inventory());
        apply_material_egress(state.inventory_state_mut(), self.egress);
        let at = self.enclosure.created_at();
        state.inventory_state_mut().apply_storage_enclosure(
            target,
            StockpileStorageProfile::unbounded_solid_only(),
            self.next_profile,
            self.enclosure,
            at,
            self.next_inventory_revision,
        );
        Ok(target)
    }
}

/// Validates placing one authored storage enclosure from the player's carried inventory at the
/// player's exact current voxel.
///
/// This is the ordinary direct-manipulation boundary. Capacity comes from the authored storage
/// definition, the exact enclosure body comes from carried custody, and the implementation-only
/// empty stockpile allocation never becomes a separate player decision. Lower-level allocation and
/// enclosure APIs remain available for world/bootstrap setup and improving existing stockpiles.
pub fn validate_place_player_storage(
    registries: &Registries,
    state: &AppState,
    definition: StorageDefinitionId,
) -> Result<ValidatedPlayerStoragePlacement, PlayerStoragePlacementError> {
    let definition_record =
        registries
            .storage()
            .get(definition)
            .ok_or(PlayerStoragePlacementError::Construction(
                StorageEnclosureConstructionError::UnknownDefinition { definition },
            ))?;
    let source = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(PlayerStoragePlacementError::Allocation(
            crate::logistics::PlayerGroundStockpileAllocationError::PlayerNotInitialized,
        ))?;
    validate_player_stockpile_access(state, source)
        .map_err(StorageEnclosureConstructionError::SourceAccess)
        .map_err(PlayerStoragePlacementError::Construction)?;
    let selection = select_enclosure_material(state, definition_record, source)
        .map_err(PlayerStoragePlacementError::Construction)?;
    let material_plan = plan_enclosure_materials(registries, state, source, selection)
        .map_err(PlayerStoragePlacementError::Construction)?;
    assert!(
        material_plan.structural_load.is_none(),
        "player-carried construction custody cannot own a structural stored-matter load"
    );
    if !state.can_spend_inventory_revisions(3) {
        return Err(PlayerStoragePlacementError::Construction(
            StorageEnclosureConstructionError::InventoryRevisionExhausted,
        ));
    }
    let egress = material_plan
        .egress
        .after_empty_stockpile_allocation()
        .map_err(|error| match error {
            MaterialEgressError::RevisionExhausted => PlayerStoragePlacementError::Construction(
                StorageEnclosureConstructionError::InventoryRevisionExhausted,
            ),
            MaterialEgressError::StaleSelection { .. } => {
                unreachable!("current enclosure selection cannot be stale before placement")
            }
        })?;
    let allocation = validate_allocate_player_ground_stockpile(
        state,
        definition_record.maximum_stockpile_capacity(),
    )
    .map_err(PlayerStoragePlacementError::Allocation)?;
    let next_inventory_revision = state
        .inventory()
        .revision()
        .checked_add(3)
        .unwrap_or_else(|| unreachable!("placement headroom includes all inventory mutations"));
    Ok(ValidatedPlayerStoragePlacement {
        allocation,
        next_inventory_revision,
        next_profile: definition_record.storage_profile(),
        enclosure: StockpileEnclosureRecord::new(
            definition,
            material_plan.embodied_material,
            state.tick(),
        ),
        egress,
    })
}

impl ValidatedStorageEnclosureConstruction {
    /// Transfers the exact selected construction matter into persistent enclosure ownership.
    pub fn commit(self, state: &mut AppState) -> Result<(), StorageEnclosureCommitError> {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.expected_inventory_revision {
            return Err(StorageEnclosureCommitError::StaleInventoryRevision {
                expected: self.expected_inventory_revision,
                actual: actual_revision,
            });
        }
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(StorageEnclosureCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        let target = state.inventory().get_stockpile(self.target).ok_or(
            StorageEnclosureCommitError::UnknownTarget {
                stockpile: self.target,
            },
        )?;
        if target.storage_profile() != self.expected_profile {
            return Err(StorageEnclosureCommitError::TargetProfileChanged {
                stockpile: self.target,
            });
        }
        if target.enclosure().is_some() {
            return Err(StorageEnclosureCommitError::TargetEnclosureChanged {
                stockpile: self.target,
            });
        }
        self.egress.assert_matches_state(state.inventory());
        if let Some(structural_load) = self.structural_load {
            structural_load
                .commit(state)
                .map_err(StorageEnclosureCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        let at = self.enclosure.created_at();
        state.inventory_state_mut().apply_storage_enclosure(
            self.target,
            self.expected_profile,
            self.next_profile,
            self.enclosure,
            at,
            self.next_inventory_revision,
        );
        Ok(())
    }
}

/// Validates enclosing one existing ambient solid stockpile with an authored material-backed store.
///
/// Construction is intentionally in-place because general world-space haulage is not implemented.
/// Existing lot exposure is checkpointed at the construction tick before the improved preservation
/// multiplier begins, so infrastructure never retroactively restores freshness.
pub fn validate_build_storage_enclosure(
    registries: &Registries,
    state: &AppState,
    definition: StorageDefinitionId,
    target: StockpileId,
    source: StockpileId,
) -> Result<ValidatedStorageEnclosureConstruction, StorageEnclosureConstructionError> {
    let definition_record = registries
        .storage()
        .get(definition)
        .ok_or(StorageEnclosureConstructionError::UnknownDefinition { definition })?;
    let target_record = state
        .inventory()
        .get_stockpile(target)
        .ok_or(StorageEnclosureConstructionError::UnknownTarget { stockpile: target })?;
    if state
        .logistics()
        .player()
        .is_some_and(|player| player.carried_stockpile() == target)
    {
        return Err(StorageEnclosureConstructionError::PlayerCarriedTarget { stockpile: target });
    }
    validate_player_stockpile_access(state, target)
        .map_err(StorageEnclosureConstructionError::TargetAccess)?;
    validate_player_stockpile_access(state, source)
        .map_err(StorageEnclosureConstructionError::SourceAccess)?;
    if state
        .player_work()
        .get_storage_dismantling_stockpile_occupant(target)
        .is_some()
    {
        return Err(
            StorageEnclosureConstructionError::TargetBusyStorageDismantling { stockpile: target },
        );
    }
    let required_profile = StockpileStorageProfile::unbounded_solid_only();
    validate_enclosure_target(definition_record, target_record, target, required_profile)?;
    validate_world_location(state, target, source)?;
    let selection = select_enclosure_material(state, definition_record, source)?;
    let next_profile = definition_record.storage_profile();
    validate_enclosure_contents(registries, state, target_record, &selection, next_profile)?;
    let material_plan = plan_enclosure_materials(registries, state, source, selection)?;
    validate_unreserved_stockpile_structural_load_headroom(
        state,
        material_plan.structural_load.as_ref(),
    )
    .map_err(StorageEnclosureConstructionError::StructuralLoad)?;
    let expected_inventory_revision = state.inventory().revision();
    if !state.can_spend_inventory_revisions(2) {
        return Err(StorageEnclosureConstructionError::InventoryRevisionExhausted);
    }
    let next_inventory_revision = expected_inventory_revision
        .checked_add(2)
        .unwrap_or_else(|| {
            unreachable!("inventory headroom check includes enclosure construction revisions")
        });
    Ok(ValidatedStorageEnclosureConstruction {
        target,
        expected_inventory_revision,
        expected_logistics_revision: state.logistics().revision(),
        next_inventory_revision,
        expected_profile: required_profile,
        next_profile,
        enclosure: StockpileEnclosureRecord::new(
            definition,
            material_plan.embodied_material,
            state.tick(),
        ),
        egress: material_plan.egress,
        structural_load: material_plan.structural_load,
    })
}

fn validate_world_location(
    state: &AppState,
    target: StockpileId,
    source: StockpileId,
) -> Result<(), StorageEnclosureConstructionError> {
    let Some(target_position) = state.logistics().stationary_stockpile_position(target) else {
        return Ok(());
    };
    let Some(source_position) = state.logistics().stockpile_position(source) else {
        return Err(
            StorageEnclosureConstructionError::LocatedTargetSourceUnlocated {
                target,
                target_position,
                source,
            },
        );
    };
    if source_position != target_position {
        return Err(
            StorageEnclosureConstructionError::LocatedTargetSourceRemote {
                target,
                target_position,
                source,
                source_position,
            },
        );
    }
    Ok(())
}

fn validate_enclosure_target(
    definition: &StorageDefinition,
    target_record: &StockpileRecord,
    target: StockpileId,
    required_profile: StockpileStorageProfile,
) -> Result<(), StorageEnclosureConstructionError> {
    if let Some(enclosure) = target_record.enclosure() {
        return Err(StorageEnclosureConstructionError::AlreadyEnclosed {
            stockpile: target,
            definition: enclosure.definition(),
        });
    }
    if let Some(element) = target_record.supported_by() {
        return Err(StorageEnclosureConstructionError::TargetMounted {
            stockpile: target,
            element,
        });
    }
    if target_record.capacity() > definition.maximum_stockpile_capacity() {
        return Err(StorageEnclosureConstructionError::TargetCapacityTooLarge {
            stockpile: target,
            capacity: target_record.capacity(),
            maximum: definition.maximum_stockpile_capacity(),
        });
    }
    if target_record.storage_profile() != required_profile {
        return Err(
            StorageEnclosureConstructionError::TargetStorageProfileMismatch {
                stockpile: target,
                current: target_record.storage_profile(),
                required: required_profile,
            },
        );
    }
    if !target_record.reserved_inbound().is_zero() {
        return Err(
            StorageEnclosureConstructionError::TargetHasReservedInbound {
                stockpile: target,
                reserved: target_record.reserved_inbound(),
            },
        );
    }
    Ok(())
}

fn select_enclosure_material(
    state: &AppState,
    definition: &StorageDefinition,
    source: StockpileId,
) -> Result<ConsumptionSelection, StorageEnclosureConstructionError> {
    validate_consumption_selection(
        state.inventory(),
        source,
        definition.assembly_profile().inputs(),
    )
    .map_err(map_selection_error)
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

fn validate_enclosure_contents(
    registries: &Registries,
    state: &AppState,
    target_record: &StockpileRecord,
    selection: &ConsumptionSelection,
    next_profile: StockpileStorageProfile,
) -> Result<(), StorageEnclosureConstructionError> {
    let target = target_record.id();
    let source = selection.source();
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

fn plan_enclosure_materials(
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

#[cfg(test)]
#[path = "enclosure_execution_tests.rs"]
mod tests;
