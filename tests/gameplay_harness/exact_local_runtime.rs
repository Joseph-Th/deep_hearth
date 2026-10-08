//! Runtime-state invariant for ordinary exact-local gameplay evidence.

use std::collections::BTreeSet;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{EnergyStoreDefinitionId, EnergyStoreId};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::logistics::{
    validate_initialize_player_logistics, validate_place_fluid_store,
    validate_place_ground_stockpile,
};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelCoord;

pub(super) const STATIONARY_PLAYER_ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);

#[allow(
    dead_code,
    reason = "exact-local targets use only the endpoint kinds relevant to their current episode"
)]
pub(super) fn unique_local_equipment(
    state: &AppState,
    definition: EquipmentDefinitionId,
    context: &'static str,
) -> EquipmentId {
    let matches = state
        .available_local_equipment()
        .filter(|record| record.definition() == definition)
        .map(|record| record.id())
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "stationary gameplay {context} expected one actor-visible local equipment instance of definition {} but found {}",
        definition.value(),
        matches.len()
    );
    matches[0]
}

#[allow(
    dead_code,
    reason = "exact-local targets use only the endpoint kinds relevant to their current episode"
)]
pub(super) fn unique_local_energy_store(
    state: &AppState,
    definition: EnergyStoreDefinitionId,
    context: &'static str,
) -> EnergyStoreId {
    let matches = state
        .available_local_energy_stores()
        .filter(|record| record.definition() == definition)
        .map(|record| record.id())
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "stationary gameplay {context} expected one actor-visible local energy store of definition {} but found {}",
        definition.value(),
        matches.len()
    );
    matches[0]
}

/// Locates pre-existing stationary endpoints at the ordinary player's workshop voxel.
///
/// Call this only after all fixture seeding for those endpoints and before runtime actions begin.
#[allow(
    dead_code,
    reason = "some focused targets only validate exact-local state and do not perform stationary fixture admission"
)]
pub(super) fn locate_stationary_endpoints(
    state: &mut AppState,
    stockpiles: &[StockpileId],
    fluid_stores: &[FluidStoreId],
) {
    for &stockpile in stockpiles {
        validate_place_ground_stockpile(state, stockpile, STATIONARY_PLAYER_ORIGIN)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay stockpile {} placement failed: {error}",
                    stockpile.value()
                )
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay stockpile {} placement commit failed: {error}",
                    stockpile.value()
                )
            });
    }
    for &store in fluid_stores {
        validate_place_fluid_store(state, store, STATIONARY_PLAYER_ORIGIN)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay fluid store {} placement failed: {error}",
                    store.value()
                )
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay fluid store {} placement commit failed: {error}",
                    store.value()
                )
            });
    }
}

/// Initializes the real player logistics owner at the stationary workshop voxel.
#[allow(
    dead_code,
    reason = "some focused targets only validate exact-local state and do not perform stationary fixture admission"
)]
pub(super) fn initialize_stationary_player_logistics(state: &mut AppState) {
    validate_initialize_player_logistics(state, STATIONARY_PLAYER_ORIGIN, Mass::from_milligrams(1))
        .unwrap_or_else(|error| {
            panic!("stationary gameplay logistics initialization failed: {error}")
        })
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("stationary gameplay logistics initialization commit failed: {error}")
        });
}

/// Verifies that one exact-local ordinary world is fully admitted to runtime state the player can
/// actually act on.
///
/// Controlled setup may create endpoints before the actor exists, but none may remain locationless
/// once ordinary player logistics is initialized.
pub(super) fn assert_exact_local_runtime_ready(
    registries: &Registries,
    state: &AppState,
    context: &'static str,
) {
    let player = state.logistics().player().unwrap_or_else(|| {
        panic!("stationary gameplay {context} has no admitted logistics player")
    });
    assert_eq!(
        player.position(),
        STATIONARY_PLAYER_ORIGIN,
        "stationary gameplay {context} player moved away from its exact-local workshop"
    );
    for stockpile in state.inventory().stockpiles() {
        assert_eq!(
            state.logistics().stockpile_position(stockpile.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} stockpile {} is not local to the admitted player",
            stockpile.id().value()
        );
    }
    for equipment in state.equipment().equipment() {
        assert_eq!(
            state.logistics().equipment_position(equipment.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} equipment {} is not local to the admitted player",
            equipment.id().value()
        );
    }
    assert_eq!(
        state
            .available_local_equipment()
            .map(|equipment| equipment.id())
            .collect::<BTreeSet<_>>(),
        state
            .equipment()
            .equipment()
            .map(|equipment| equipment.id())
            .collect::<BTreeSet<_>>(),
        "stationary gameplay {context} actor-visible equipment diverged from exact-local runtime equipment"
    );
    for store in state.energy().stores() {
        assert_eq!(
            state.logistics().energy_store_position(store.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} energy store {} is not local to the admitted player",
            store.id().value()
        );
    }
    assert_eq!(
        state
            .available_local_energy_stores()
            .map(|store| store.id())
            .collect::<BTreeSet<_>>(),
        state
            .energy()
            .stores()
            .map(|store| store.id())
            .collect::<BTreeSet<_>>(),
        "stationary gameplay {context} actor-visible energy stores diverged from exact-local runtime energy stores"
    );
    for store in state.fluid().stores() {
        assert_eq!(
            state.logistics().fluid_store_position(store.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} fluid store {} is not local to the admitted player",
            store.id().value()
        );
    }
    assert_eq!(
        state
            .available_local_fluid_stores()
            .map(|store| store.id())
            .collect::<BTreeSet<_>>(),
        state
            .fluid()
            .stores()
            .map(|store| store.id())
            .collect::<BTreeSet<_>>(),
        "stationary gameplay {context} actor-visible fluid stores diverged from exact-local runtime fluid stores"
    );
    assert_eq!(
        state
            .available_local_ground_stockpiles()
            .map(|stockpile| stockpile.id())
            .collect::<BTreeSet<_>>(),
        state
            .logistics()
            .stockpile_locations()
            .map(|(stockpile, _)| stockpile)
            .collect::<BTreeSet<_>>(),
        "stationary gameplay {context} actor-visible ground stockpiles diverged from exact-local runtime custody"
    );
    validate_loaded_state(registries, state).unwrap_or_else(|error| {
        panic!("stationary gameplay {context} admitted invalid runtime state: {error}")
    });
}
