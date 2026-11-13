//! Material-backed construction of preservation enclosures around existing stockpiles.

use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::logistics::{
    ValidatedGroundStockpileAllocation, validate_allocate_player_ground_stockpile_at,
    validate_player_stockpile_access,
};
use crate::player::{PlayerActionGuard, validate_optional_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM, exposed_residue_compact_depth_um};

use super::{
    MaterialEgressError, StockpileEnclosureRecord, StockpileId, StockpileRecord,
    StockpileStorageProfile, StorageDefinition, StorageDefinitionId, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_unreserved_stockpile_structural_load_headroom,
};

mod errors;
mod materials;
mod site;

use materials::{plan_enclosure_materials, select_enclosure_material, validate_enclosure_contents};
use site::{
    enclosure_position_is_solid, generated_enclosure_has_ground_support, rooted_foraging_at,
    validate_world_location,
};

pub use errors::{
    PlayerStoragePlacementCommitError, PlayerStoragePlacementError, StorageEnclosureCommitError,
    StorageEnclosureConstructionError,
};

/// Revision-bound proof that exact construction matter can become one stockpile enclosure.
#[must_use]
pub struct ValidatedStorageEnclosureConstruction {
    player_action: PlayerActionGuard,
    expected_control_tick: ControlTick,
    expected_combat_revision: u64,
    target: StockpileId,
    expected_inventory_revision: u64,
    expected_logistics_revision: u64,
    expected_surface_revision: u64,
    expected_world_revision: u64,
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
    expected_control_tick: ControlTick,
    expected_combat_revision: u64,
    expected_surface_revision: u64,
    expected_world_revision: u64,
    allocation: ValidatedGroundStockpileAllocation,
    next_inventory_revision: u64,
    next_profile: StockpileStorageProfile,
    enclosure: StockpileEnclosureRecord,
    egress: ValidatedMaterialEgress,
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
        if state.player_work().active().is_some() {
            return Err(PlayerStoragePlacementCommitError::PlayerBecameBusy);
        }
        let actual_combat_revision = state.combat().revision();
        if actual_combat_revision != self.expected_combat_revision {
            return Err(PlayerStoragePlacementCommitError::StaleCombatRevision {
                expected: self.expected_combat_revision,
                actual: actual_combat_revision,
            });
        }
        if state.surface().revision() != self.expected_surface_revision {
            return Err(PlayerStoragePlacementCommitError::SurfaceRevisionChanged {
                expected: self.expected_surface_revision,
                actual: state.surface().revision(),
            });
        }
        if state.world().revision() != self.expected_world_revision {
            return Err(PlayerStoragePlacementCommitError::WorldRevisionChanged {
                expected: self.expected_world_revision,
                actual: state.world().revision(),
            });
        }
        if state.control_tick() != self.expected_control_tick {
            return Err(PlayerStoragePlacementCommitError::StaleControlTick {
                expected: self.expected_control_tick,
                actual: state.control_tick(),
            });
        }
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

/// Validates placing one authored storage enclosure from the player's carried inventory at an
/// already-authorized world voxel.
///
/// The caller owns spatial authorization for `position`; this owner retains matter/custody,
/// survival, revision, and enclosure construction authority.
pub(crate) fn validate_place_player_storage_at(
    registries: &Registries,
    state: &AppState,
    definition: StorageDefinitionId,
    position: VoxelCoord,
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
    if state.player_work().active().is_some() {
        return Err(PlayerStoragePlacementError::PlayerBusy);
    }
    if state.combat().has_prepared_strike() {
        return Err(PlayerStoragePlacementError::PreparedStrikeActive);
    }
    validate_player_stockpile_access(state, source)
        .map_err(StorageEnclosureConstructionError::SourceAccess)
        .map_err(PlayerStoragePlacementError::Construction)?;
    if enclosure_position_is_solid(registries, state, position) {
        return Err(PlayerStoragePlacementError::GeneratedSolidTerrain { position });
    }
    if rooted_foraging_at(state, position) {
        return Err(PlayerStoragePlacementError::RootedVegetationOccupiesSite { position });
    }
    // Ordinary pointed placement already excludes residents. The owner-level
    // placement capability must enforce the same physical clearance so that
    // direct callers cannot construct persistent matter around a living actor.
    if crate::kobolds::opening_kobold_occupies_voxel(state, position)
        || crate::kobolds::temple_caste_occupies_voxel(registries, state, position)
    {
        return Err(PlayerStoragePlacementError::ResidentOccupiesSite { position });
    }
    let residue_depth = exposed_residue_compact_depth_um(registries, state, position);
    if residue_depth >= OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM {
        return Err(PlayerStoragePlacementError::LooseResidueFooting {
            position,
            compact_depth_um: residue_depth,
        });
    }
    // Building a new enclosure must meet the same floor rule as enclosing an
    // existing stockpile. Otherwise direct placement can freeze built matter in
    // midair even though a constructed stockpile at that site would be rejected.
    if !generated_enclosure_has_ground_support(registries, state, position) {
        return Err(PlayerStoragePlacementError::Construction(
            StorageEnclosureConstructionError::UnsupportedFloor { position },
        ));
    }
    let allocation = validate_allocate_player_ground_stockpile_at(
        state,
        position,
        definition_record.maximum_stockpile_capacity(),
    )
    .map_err(PlayerStoragePlacementError::Allocation)?;
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
    let next_inventory_revision = state
        .inventory()
        .revision()
        .checked_add(3)
        .unwrap_or_else(|| unreachable!("placement headroom includes all inventory mutations"));
    Ok(ValidatedPlayerStoragePlacement {
        expected_control_tick: state.control_tick(),
        expected_combat_revision: state.combat().revision(),
        expected_surface_revision: state.surface().revision(),
        expected_world_revision: state.world().revision(),
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

/// Validates placing one authored storage enclosure from the player's carried inventory at the
/// player's exact current voxel.
///
/// Direct pointed interaction should prefer the crate-owned exact-position boundary after proving
/// its target. This wrapper remains the owner-level local-placement capability for callers whose
/// action is explicitly defined as occurring at the player's current voxel.
pub fn validate_place_player_storage(
    registries: &Registries,
    state: &AppState,
    definition: StorageDefinitionId,
) -> Result<ValidatedPlayerStoragePlacement, PlayerStoragePlacementError> {
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .ok_or(PlayerStoragePlacementError::Allocation(
            crate::logistics::PlayerGroundStockpileAllocationError::PlayerNotInitialized,
        ))?;
    validate_place_player_storage_at(registries, state, definition, position)
}

impl ValidatedStorageEnclosureConstruction {
    /// Transfers the exact selected construction matter into persistent enclosure ownership.
    pub fn commit(self, state: &mut AppState) -> Result<(), StorageEnclosureCommitError> {
        if state.player_work().active().is_some() {
            return Err(StorageEnclosureCommitError::PlayerBecameBusy);
        }
        let actual_combat_revision = state.combat().revision();
        if actual_combat_revision != self.expected_combat_revision {
            return Err(StorageEnclosureCommitError::StaleCombatRevision {
                expected: self.expected_combat_revision,
                actual: actual_combat_revision,
            });
        }
        let actual_revision = state.inventory().revision();
        if actual_revision != self.expected_inventory_revision {
            return Err(StorageEnclosureCommitError::StaleInventoryRevision {
                expected: self.expected_inventory_revision,
                actual: actual_revision,
            });
        }
        self.player_action.precheck(state).map_err(|stale| {
            StorageEnclosureCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(StorageEnclosureCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        // The foundation and rooted vegetation are surface-owned. A concurrent
        // spill or growth must invalidate this already-admitted construction.
        let actual_surface_revision = state.surface().revision();
        if actual_surface_revision != self.expected_surface_revision {
            return Err(StorageEnclosureCommitError::StaleSurfaceRevision {
                expected: self.expected_surface_revision,
                actual: actual_surface_revision,
            });
        }
        let actual_world_revision = state.world().revision();
        if actual_world_revision != self.expected_world_revision {
            return Err(StorageEnclosureCommitError::StaleWorldRevision {
                expected: self.expected_world_revision,
                actual: actual_world_revision,
            });
        }
        // Resident position is not an inventory/logistics revision. Every
        // normal scout or temple step advances the control clock, so a delayed
        // construction token must never materialize a box at an old location.
        if state.control_tick() != self.expected_control_tick {
            return Err(StorageEnclosureCommitError::StaleControlTick {
                expected: self.expected_control_tick,
                actual: state.control_tick(),
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
/// Construction consumes staged material at the target's exact location. Player carrying and
/// ground transfers move stock between sites; enclosure construction does not haul remote inputs.
/// Existing lot exposure is checkpointed at the construction tick before the improved preservation
/// multiplier begins, so infrastructure never retroactively restores freshness.
pub fn validate_build_storage_enclosure(
    registries: &Registries,
    state: &AppState,
    definition: StorageDefinitionId,
    target: StockpileId,
    source: StockpileId,
) -> Result<ValidatedStorageEnclosureConstruction, StorageEnclosureConstructionError> {
    let player_action = validate_optional_player_action(state)
        .map_err(|_| StorageEnclosureConstructionError::PlayerDead)?;
    if state.player_work().active().is_some() {
        return Err(StorageEnclosureConstructionError::PlayerBusy);
    }
    if state.logistics().player().is_some() && state.combat().has_prepared_strike() {
        return Err(StorageEnclosureConstructionError::PreparedStrikeActive);
    }
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
    validate_world_location(registries, state, target, source)?;
    if let Some(position) = state.logistics().stationary_stockpile_position(target)
        && (crate::kobolds::opening_kobold_occupies_voxel(state, position)
            || crate::kobolds::temple_caste_occupies_voxel(registries, state, position))
    {
        return Err(StorageEnclosureConstructionError::ResidentOccupiesSite { position });
    }
    let selection = select_enclosure_material(state, definition_record, source)?;
    let next_profile = definition_record.storage_profile();
    validate_enclosure_contents(
        registries,
        state,
        target_record,
        &selection,
        definition_record,
    )?;
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
        player_action,
        expected_control_tick: state.control_tick(),
        expected_combat_revision: state.combat().revision(),
        target,
        expected_inventory_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_surface_revision: state.surface().revision(),
        expected_world_revision: state.world().revision(),
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

#[cfg(test)]
#[path = "enclosure_execution_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "enclosure_footing_tests.rs"]
mod footing_tests;

#[cfg(test)]
#[path = "enclosure_site_tests.rs"]
mod site_tests;
