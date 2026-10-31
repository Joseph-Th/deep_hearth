//! Real forage acquisition and persistence across generated-world ownership.

use super::*;

use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, build_registries};
use crate::core::quantity::Mass;
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::surface::{
    SurfaceResourceKind, player_surface_gathering_options,
    validate_start_player_surface_gathering_option,
};
use crate::world::validate_generate_chunk;

#[test]
fn generated_forage_is_one_local_physical_opportunity_not_an_infinite_refill() {
    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(0x46_4F_52_41_47_45_57);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        super::super::ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(
        state.surface().resources().next().is_none(),
        "terrain-only bootstrap must not preempt opening wilderness"
    );
    let spawn = VoxelCoord::new(0, super::super::terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let demand = super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("admitted player needs the surrounding wilderness");
    assert_eq!(demand.chunks().count(), 8);
    let _ = demand.commit(&mut state).unwrap();
    let sources = state
        .surface()
        .resources()
        .filter(|record| record.kind() == SurfaceResourceKind::Opportunity)
        .collect::<Vec<_>>();
    assert!(
        !sources.is_empty(),
        "player-neighborhood expansion must reveal finite wild food"
    );
    let patch = sources[0];
    assert_eq!(patch.commodity().form(), FORM_FOOD);
    assert!(patch.remaining_mass() >= Mass::from_milligrams(250_000));
    let position = patch.position();
    let source_id = patch.id();
    let starting = patch.remaining_mass();
    assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
    assert_eq!(
        state.world().block_at(VoxelCoord::new(
            position.x(),
            position.y() - 1,
            position.z()
        )),
        Some(BLOCK_STATE_STONE)
    );
    let total_before = calculate_matter_accounting(&state).unwrap().total();

    crate::logistics::relocate_player_for_test(&mut state, position);
    assert!(
        crate::surface::AdjacentSurfaceResource::visible_in_generated_world(&registries, &state)
            .any(|sighting| sighting.resource() == source_id),
        "a natural patch must be locally observable through the production read surface"
    );
    // Look down the generated row from two cells away. Rock placed in
    // the intervening cell must conceal the patch, not advertise its ID.
    let mut visibility = state.clone();
    let middle = VoxelCoord::new(position.x() + 1, position.y(), position.z());
    let observer = VoxelCoord::new(position.x() + 2, position.y(), position.z());
    for cell in [
        middle,
        observer,
        VoxelCoord::new(observer.x(), observer.y() + 1, observer.z()),
    ] {
        crate::world::set_generated_block_for_test(
            registries.world(),
            &mut visibility,
            cell,
            BlockStateId::AIR,
        );
    }
    crate::logistics::relocate_player_for_test(&mut visibility, observer);
    assert!(
        crate::surface::AdjacentSurfaceResource::visible_in_generated_world(
            &registries,
            &visibility
        )
        .any(|sighting| sighting.resource() == source_id),
        "two-cell unobstructed local evidence should be visible"
    );
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut visibility,
        middle,
        BLOCK_STATE_STONE,
    );
    assert!(
        crate::surface::AdjacentSurfaceResource::visible_in_generated_world(
            &registries,
            &visibility
        )
        .all(|sighting| sighting.resource() != source_id),
        "an intervening opaque block must hide the finite source"
    );
    let option = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.resource() == source_id)
        .expect("the generated patch must admit an ordinary player gathering method");
    let food = option.food_value(&registries, &state).unwrap().unwrap();
    assert!(food.dietary_energy().nanojoules() > 0);
    let batch = Mass::from_milligrams(25_000);
    validate_start_player_surface_gathering_option(&registries, &state, option, batch)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..300 {
        if state.player_work().active().is_none() {
            break;
        }
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.player_work().active().is_none(),
        "gathering must finish"
    );
    let source = state.surface().get(source_id).unwrap();
    assert_eq!(
        source.remaining_mass(),
        starting.checked_sub(batch).unwrap()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        total_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &restored,
            seed,
            super::super::ChunkCoord::containing(position),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(super::super::ChunkGenerationError::AlreadyGenerated {
            chunk: super::super::ChunkCoord::containing(position)
        }),
        "revisiting the generated chunk cannot restore harvested food"
    );
}

#[test]
fn forage_neighborhood_batch_is_deterministic_and_does_not_reset_age() {
    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(0x464F_5241_4745_434F);
    let generate = |state: &mut AppState| {
        validate_generate_chunk(
            &registries,
            state,
            seed,
            super::super::ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(state)
        .unwrap();
        let spawn = VoxelCoord::new(0, super::super::terrain_height(seed, 0, 0) + 1, 0);
        validate_admit_player(&registries, state, spawn)
            .unwrap()
            .commit(state)
            .unwrap();
        let demand = super::super::validate_generate_player_neighborhood(&registries, state)
            .unwrap()
            .expect("seeded neighborhood must have ungenerated chunks");
        assert_eq!(demand.chunks().count(), 8);
        let _ = demand.commit(state).unwrap();
        assert!(
            super::super::validate_generate_player_neighborhood(&registries, state)
                .unwrap()
                .is_none(),
            "revisiting the same neighborhood must not regenerate the food"
        );
    };
    let mut immediate = AppState::new();
    generate(&mut immediate);
    let observed = |state: &AppState| {
        let mut patches = state
            .surface()
            .resources()
            .filter(|resource| resource.kind() == SurfaceResourceKind::Opportunity)
            .map(|resource| {
                (
                    resource.position(),
                    resource.commodity(),
                    resource.remaining_mass(),
                )
            })
            .collect::<Vec<_>>();
        patches.sort_unstable();
        patches
    };
    assert!(!observed(&immediate).is_empty());
    assert!(calculate_matter_accounting(&immediate).is_ok());
    assert_eq!(validate_loaded_state(&registries, &immediate), Ok(()));

    let mut delayed = AppState::new();
    for _ in 0..100 {
        let _ = advance_tick(&registries, &mut delayed).unwrap();
    }
    assert_eq!(delayed.tick(), SimulationTick::new(100));
    generate(&mut delayed);
    assert_eq!(observed(&immediate), observed(&delayed));
    assert!(
        delayed
            .surface()
            .resources()
            .all(|resource| resource.created_at() == SimulationTick::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &delayed), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &delayed)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, delayed);
}
#[test]
fn competing_surface_custody_invalidates_entire_neighborhood_generation() {
    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(0x464F_5241_4745_434F);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        super::super::ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let spawn = VoxelCoord::new(0, super::super::terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let demand = super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("the new wilderness needs a generated neighborhood");
    let spec = crate::surface::SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        spawn,
        crate::material::CommodityKey::new(crate::content::MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        crate::core::quantity::Temperature::from_millikelvin(293_150),
        crate::material::MaterialComposition::pure(crate::content::MATERIAL_BERRIES),
    )
    .unwrap();
    crate::surface::validate_create_surface_resources(&registries, &state, [spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        demand.commit(&mut state),
        Err(
            super::super::ChunkGenerationCommitError::StaleSurfaceRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(
        state, before,
        "a stale multi-owner generation must mutate neither owner"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
