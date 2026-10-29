//! Kobold opening patrol, physical obstruction, local observation, and continuation contracts.

use crate::content::{FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};
use crate::world::WorldGenerationSeed;
use crate::world_generation::validate_generate_initial_wilderness;

use super::{KoboldColony, available_nearby_kobold_scouts, validate_loaded_kobolds};

#[path = "tests/caches_and_detours.rs"]
mod caches_and_detours;
#[path = "tests/cargo_and_recovery.rs"]
mod cargo_and_recovery;
#[path = "tests/food_and_den.rs"]
mod food_and_den;
#[path = "tests/patrol_visibility_and_terrain.rs"]
mod patrol_visibility_and_terrain;
#[path = "tests/world_and_guard.rs"]
mod world_and_guard;

fn colony(state: &AppState) -> &KoboldColony {
    state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap_or_else(|| panic!("opening patrol missing"))
}

fn generated_opening() -> (crate::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    let spawn = VoxelCoord::new(8, 2, 8);
    validate_generate_initial_wilderness(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44),
        spawn,
    )
    .expect("opening colony should be admitted with wilderness")
    .commit(&mut state)
    .expect("opening colony should commit atomically");
    (registries, state, spawn)
}

/// Gather every locally observed nut through the same player work owner used
/// in gameplay. The real remaining surface source, not a flag on the scout,
/// then drives its decision to seek the neighboring berry opportunity.
fn exhaust_junction_nuts(
    registries: &crate::Registries,
    state: &mut AppState,
) -> (VoxelCoord, VoxelCoord, crate::surface::SurfaceResourceId) {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
    use crate::surface::{
        player_surface_gathering_options, validate_start_player_surface_gathering_option,
    };
    let junction = colony(state).route[1];
    let forage = state
        .logistics()
        .traversable_destinations(junction)
        .find(|position| {
            state
                .surface()
                .available_resources_at(Some(*position))
                .any(|resource| {
                    resource.commodity() == CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
                })
        })
        .expect("opening provides an adjacent berry opportunity");
    let source = state
        .surface()
        .available_resources_at(Some(forage))
        .find(|resource| resource.commodity() == CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .unwrap()
        .id();
    let nuts = state
        .surface()
        .available_resources_at(Some(junction))
        .find(|resource| resource.commodity() == CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD))
        .unwrap()
        .id();
    validate_admit_player(registries, state, junction)
        .unwrap()
        .commit(state)
        .unwrap();
    while state.surface().get(nuts).unwrap().remaining_mass() != Mass::ZERO {
        let option = player_surface_gathering_options(registries, state)
            .unwrap()
            .into_iter()
            .find(|option| option.resource() == nuts)
            .unwrap();
        let batch = state
            .surface()
            .get(nuts)
            .unwrap()
            .remaining_mass()
            .min(option.maximum_batch_mass());
        validate_start_player_surface_gathering_option(registries, state, option, batch)
            .unwrap()
            .commit(state)
            .unwrap();
        for _ in 0..100 {
            if state.player_work().active().is_none() {
                break;
            }
            let _ = advance_tick(registries, state).unwrap();
        }
        assert!(
            state.player_work().active().is_none(),
            "authoritative surface gathering must finish before the scout arrives"
        );
    }
    // Graph distance is not physical proximity: clay may be diagonally adjacent
    // to the scout's clearing. Owner-level setup must put the observer beyond the
    // actual local fear radius for a test of unguarded foraging.
    let water = state
        .logistics()
        .traversable_destinations(forage)
        .find(|position| {
            *position != junction
                && state.logistics().has_traversal_edge(
                    *position,
                    state
                        .logistics()
                        .traversable_destinations(*position)
                        .find(|next| *next != forage)
                        .unwrap(),
                )
        })
        .expect("opening berry branch reaches the water cell");
    let clay = state
        .logistics()
        .traversable_destinations(water)
        .find(|position| *position != forage)
        .unwrap();
    crate::logistics::relocate_player_for_test(
        state,
        VoxelCoord::new(clay.x() + 4, clay.y(), clay.z() + 4),
    );
    (junction, forage, source)
}

fn place_loose_stone_drop(
    registries: &crate::Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> crate::surface::SurfaceResourceId {
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .expect("stone chips form finite loose material");
    let plan = validate_create_surface_resources(registries, state, vec![spec])
        .expect("stone chips should have valid surface custody");
    let id = plan.first_resource_id();
    plan.commit(state).expect("stone chips should commit");
    id
}

fn advance_to_tick(registries: &crate::Registries, state: &mut AppState, tick: u64) {
    while state.tick().value() < tick {
        let _ = advance_tick(registries, state).expect("kobold encounter slow tick failed");
    }
    assert_eq!(state.tick().value(), tick);
}

fn generated_terrain_scout_opening() -> (crate::Registries, AppState, VoxelCoord) {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, validate_generate_chunk};
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let origin = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(origin),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(8, y, 8)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let spawn = VoxelCoord::new(8, surface_y + 1, 8);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, spawn)
}
