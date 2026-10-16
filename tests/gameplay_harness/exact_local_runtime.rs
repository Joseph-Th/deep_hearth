//! Runtime-state invariant for ordinary exact-local gameplay evidence.

use std::collections::BTreeSet;

use deep_hearth::content::BLOCK_STATE_STONE;
use deep_hearth::content::gameplay_fixture::{locate_fluid_store, locate_ground_stockpile};
use deep_hearth::core::state::AppState;
use deep_hearth::energy::{EnergyStoreDefinitionId, EnergyStoreId};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::logistics::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerControlInput,
    assess_player_carrying, project_player_adjacent_walk,
};
use deep_hearth::persistence::validate_persistable_state;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::{advance_control_tick, advance_control_tick_with_input};
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::survival::assess_survival;
use deep_hearth::world::{
    CHUNK_SIDE, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    validate_generate_chunk,
};
use deep_hearth::world_generation::validate_generate_initial_wilderness;

pub(super) const STATIONARY_PLAYER_ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);

/// Generates one deterministic terrain chunk and returns a center-first surface spawn accepted by
/// the production terrain-aware wilderness layout validator.
#[allow(
    dead_code,
    reason = "shared exact-local runtime is included by focused targets that do not generate wilderness terrain"
)]
pub(super) fn generated_wilderness_surface_spawn(
    registries: &Registries,
    state: &mut AppState,
    seed: WorldGenerationSeed,
) -> VoxelCoord {
    let horizontal = VoxelCoord::new(8, 0, 8);
    let chunk = ChunkCoord::containing(horizontal);
    validate_generate_chunk(registries, state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("gameplay wilderness terrain generation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay wilderness terrain commit failed: {error}"));
    let min_x = chunk
        .world_min_x()
        .unwrap_or_else(|| panic!("gameplay wilderness initial chunk x coordinate overflowed"));
    let min_z = chunk
        .world_min_z()
        .unwrap_or_else(|| panic!("gameplay wilderness initial chunk z coordinate overflowed"));
    let side =
        i64::try_from(CHUNK_SIDE).unwrap_or_else(|_| unreachable!("authored chunk side fits i64"));
    let center = side / 2;
    let mut candidates = (0..side)
        .flat_map(|local_x| (0..side).map(move |local_z| (local_x, local_z)))
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|&(local_x, local_z)| {
        (
            (local_x - center).abs() + (local_z - center).abs(),
            local_x,
            local_z,
        )
    });
    candidates
        .into_iter()
        .find_map(|(local_x, local_z)| {
            let x = min_x.checked_add(local_x)?;
            let z = min_z.checked_add(local_z)?;
            let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).rev().find(|&y| {
                state.world().block_at(VoxelCoord::new(x, y, z)) == Some(BLOCK_STATE_STONE)
            })?;
            let spawn = VoxelCoord::new(x, surface_y.checked_add(1)?, z);
            validate_generate_initial_wilderness(registries, state, seed, spawn)
                .is_ok()
                .then_some(spawn)
        })
        .unwrap_or_else(|| {
            panic!("generated initial chunk has no terrain-valid bounded wilderness spawn")
        })
}

/// Executes one horizontal face-adjacent continuous-body leg and settles its exact locomotion cost.
#[allow(
    dead_code,
    reason = "shared exact-local runtime is included by focused targets that do not exercise locomotion"
)]
pub(super) fn walk_continuous_body_adjacent_and_settle(
    registries: &Registries,
    state: &mut AppState,
    destination: VoxelCoord,
    context: &str,
) -> u64 {
    assert!(
        state.control_tick().is_simulation_boundary(),
        "ordinary gameplay {context} continuous leg must begin at a settled slow boundary"
    );
    let origin = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("ordinary gameplay {context} lost admitted player"));
    let survival_before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("ordinary gameplay {context} lost survival before movement"));
    let projection =
        project_player_adjacent_walk(registries, state, destination).unwrap_or_else(|error| {
            panic!("ordinary gameplay {context} walk projection failed: {error}")
        });
    let x = i32::try_from(destination.x() - origin.x())
        .unwrap_or_else(|_| unreachable!("adjacent gameplay x delta fits i32"))
        * CONTROL_INPUT_PARTS_PER_MILLION;
    let z = i32::try_from(destination.z() - origin.z())
        .unwrap_or_else(|_| unreachable!("adjacent gameplay z delta fits i32"))
        * CONTROL_INPUT_PARTS_PER_MILLION;
    let horizontal = HorizontalControlInput::new(x, z).unwrap_or_else(|error| {
        panic!("ordinary gameplay {context} movement input failed: {error}")
    });
    let movement = PlayerControlInput::new(horizontal);
    let mut control_ticks = 0_u64;
    while state
        .logistics()
        .player()
        .is_some_and(|player| player.position() != destination)
    {
        assert!(
            control_ticks < 80,
            "ordinary gameplay {context} did not reach destination"
        );
        let _ = advance_control_tick_with_input(registries, state, movement)
            .unwrap_or_else(|error| panic!("ordinary gameplay {context} movement failed: {error}"));
        control_ticks += 1;
    }
    while state.logistics().player().is_some_and(|player| {
        let velocity = player.body().horizontal_velocity();
        velocity.x_nanometers_per_second() != 0 || velocity.z_nanometers_per_second() != 0
    }) {
        assert!(
            control_ticks < 100,
            "ordinary gameplay {context} did not settle"
        );
        let _ = advance_control_tick(registries, state)
            .unwrap_or_else(|error| panic!("ordinary gameplay {context} braking failed: {error}"));
        control_ticks += 1;
        assert_eq!(
            state.logistics().player().map(|player| player.position()),
            Some(destination),
            "ordinary gameplay {context} braking overshot its destination"
        );
    }
    assert_eq!(
        control_ticks,
        projection.control_ticks(),
        "ordinary gameplay {context} control execution diverged from its projection"
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(registries, state).unwrap_or_else(|error| {
            panic!("ordinary gameplay {context} settlement failed: {error}")
        });
        assert_eq!(
            state.logistics().player().map(|player| player.position()),
            Some(destination),
            "ordinary gameplay {context} settlement wait changed locality"
        );
    }
    let survival_after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("ordinary gameplay {context} lost survival after movement"));
    assert_eq!(
        survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy()),
        Some(projection.resource_budget().metabolic_energy()),
        "ordinary gameplay {context} did not settle projected energy"
    );
    assert_eq!(
        survival_before
            .hydration()
            .checked_sub(survival_after.hydration()),
        Some(projection.resource_budget().hydration()),
        "ordinary gameplay {context} did not settle projected hydration"
    );
    control_ticks
}

fn assert_trusted_persistence_ready(
    registries: &Registries,
    state: &AppState,
    context: &'static str,
) {
    validate_persistable_state(registries, state)
        .unwrap_or_else(|error| panic!("ordinary gameplay {context} trusted load failed: {error}"));
}

/// Verifies complete ordinary player admission and trusted-load validity without requiring every
/// world endpoint to occupy one stationary workshop voxel.
///
/// Moving exact-local episodes use this boundary. The stricter stationary helper below remains for
/// workshop-style episodes whose evidence specifically depends on all endpoints being co-located.
#[allow(
    dead_code,
    reason = "shared exact-local harness targets compile only the episode helpers they exercise"
)]
pub(super) fn assert_ordinary_admitted_runtime_ready(
    registries: &Registries,
    state: &AppState,
    context: &'static str,
) {
    let player = state
        .logistics()
        .player()
        .unwrap_or_else(|| panic!("ordinary gameplay {context} has no admitted logistics player"));
    assert!(
        state.survival().player().is_some(),
        "ordinary gameplay {context} has logistics without survival admission"
    );
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("ordinary gameplay {context} has no carried custody"));
    assert_eq!(
        carrying.position(),
        player.position(),
        "ordinary gameplay {context} carrying observation disagrees with player locality"
    );
    assert_trusted_persistence_ready(registries, state, context);
}

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
        locate_ground_stockpile(state, stockpile, STATIONARY_PLAYER_ORIGIN);
    }
    for &store in fluid_stores {
        locate_fluid_store(state, store, STATIONARY_PLAYER_ORIGIN);
    }
}

/// Verifies that one exact-local ordinary world is fully admitted to runtime state the player can
/// actually act on.
///
/// Controlled setup may create endpoints before the actor exists, but none may remain locationless
/// once ordinary player admission commits.
#[allow(
    dead_code,
    reason = "moving exact-local targets use the admission-only invariant while stationary targets use this stricter form"
)]
pub(super) fn assert_exact_local_runtime_ready(
    registries: &Registries,
    state: &AppState,
    context: &'static str,
) {
    assert_ordinary_admitted_runtime_ready(registries, state, context);
    let player = state.logistics().player().unwrap_or_else(|| {
        unreachable!("ordinary exact-local admission proved the logistics player exists")
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
}
