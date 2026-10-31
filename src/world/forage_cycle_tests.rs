//! Real seeded woodland regrowth, material budget, shelter, and continuation.

use super::*;

use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::validate_loaded_state;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceKind;
use crate::world::{
    ChunkCoord, WorldGenerationSeed, validate_generate_chunk, validate_generate_player_neighborhood,
};

fn generated_plant() -> (Registries, AppState, crate::surface::SurfaceResourceId) {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x46_4F_52_41_47_45_57);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let spawn_y = (crate::world::WORLD_MIN_Y..crate::world::WORLD_MAX_Y_EXCLUSIVE - 1)
        .find(|y| {
            state.world().block_at(VoxelCoord::new(0, *y, 0))
                == Some(crate::world::BlockStateId::AIR)
                && state.world().block_at(VoxelCoord::new(0, *y - 1, 0)) == Some(BLOCK_STATE_STONE)
        })
        .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, spawn_y, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let plant = state
        .surface()
        .resources()
        .find(|record| record.kind() == SurfaceResourceKind::Opportunity)
        .expect("seeded neighborhood contains rooted wild food");
    assert!(!plant.perennial_root_mass().is_zero());
    let id = plant.id();
    (registries, state, id)
}

fn strip_plant(state: &mut AppState, id: crate::surface::SurfaceResourceId) {
    let mass = state.surface().get(id).unwrap().remaining_mass();
    let next = state.surface().revision() + 1;
    let at = state.tick();
    state
        .surface_state_mut()
        .apply_gathering(id, mass, at, next);
}

#[test]
fn perennial_field_index_excludes_unrelated_drops_and_survives_depletion_and_load() {
    use crate::content::{FORM_CHIP, MATERIAL_STONE};
    use crate::core::quantity::Temperature;
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::surface::{SurfaceResourceSpec, validate_create_surface_resources};

    let (registries, mut state, plant) = generated_plant();
    let original_ids = state
        .surface()
        .resources()
        .filter(|source| source.perennial.is_some())
        .map(|source| source.id())
        .collect::<Vec<_>>();
    assert!(original_ids.contains(&plant));
    let indexed = |state: &AppState| {
        state
            .surface()
            .perennial_resources()
            .map(|source| source.id())
            .collect::<Vec<_>>()
    };
    assert_eq!(indexed(&state), original_ids);

    let position = state.surface().get(plant).unwrap().position();
    assert_ne!(position, state.logistics().player().unwrap().position());
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let drop = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        commodity,
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [drop])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        indexed(&state),
        original_ids,
        "a new drop is not a rooted plant"
    );
    strip_plant(&mut state, plant);
    assert_eq!(
        indexed(&state),
        original_ids,
        "depleted roots must remain indexed"
    );

    let original_matter = calculate_matter_accounting(&state).unwrap().total();
    for _ in 0..400 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(indexed(&state), original_ids);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(indexed(&replay), original_ids);
    assert_eq!(replay, state);
    for _ in 0..1_600 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut replay).unwrap();
    }
    assert_eq!(indexed(&state), original_ids);
    assert_eq!(
        state, replay,
        "field continuation must survive index reconstruction"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn harvested_perennial_reappears_with_smaller_real_crop_and_no_minted_matter() {
    let (registries, mut state, id) = generated_plant();
    let first = state.surface().get(id).unwrap().remaining_mass();
    let original_reserve = state.surface().get(id).unwrap().perennial_root_mass();
    strip_plant(&mut state, id);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let mut ripe = false;
    for _ in 0..4_000 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state.surface().get(id).unwrap().lifecycle() == SurfaceResourceLifecycle::Available {
            ripe = true;
            break;
        }
    }
    assert!(
        ripe,
        "depleted real woodland forage should form another crop"
    );
    let regrown = state.surface().get(id).unwrap();
    assert!(regrown.remaining_mass() < first);
    assert_eq!(
        regrown
            .remaining_mass()
            .checked_add(regrown.perennial_root_mass())
            .unwrap(),
        original_reserve,
        "only previously held underground biomass may enter the new crop"
    );
    assert!(regrown.ambient_storage_age_parts_at(state.tick()).unwrap() < 100_000_000);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let json = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&json)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..500 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut loaded).unwrap();
    }
    assert_eq!(state, loaded);
}

#[test]
fn a_covered_depleted_plant_cannot_ripen_from_hidden_roots() {
    let (registries, mut state, id) = generated_plant();
    let position = state.surface().get(id).unwrap().position();
    strip_plant(&mut state, id);
    let roof = VoxelCoord::new(position.x(), position.y() + 3, position.z());
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        roof,
        BLOCK_STATE_STONE,
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    for _ in 0..3_200 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.surface().get(id).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn untouched_fruit_wither_and_return_as_later_ripe_food_without_new_mass() {
    let (registries, mut state, id) = generated_plant();
    let original = calculate_matter_accounting(&state).unwrap().total();
    let initial_fruit = state.surface().get(id).unwrap().remaining_mass();
    let mut withered = false;
    let mut recovered = false;
    for _ in 0..8_000 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let source = state.surface().get(id).unwrap();
        if source.lifecycle() == SurfaceResourceLifecycle::Depleted {
            withered = true;
        }
        if withered && source.lifecycle() == SurfaceResourceLifecycle::Available {
            recovered = true;
            assert!(source.remaining_mass() < initial_fruit);
            assert!(
                source.ambient_storage_age_parts_at(state.tick()).unwrap() < 100_000_000,
                "new fruit must not retain the original old crop's stale storage age"
            );
            break;
        }
    }
    assert!(
        withered,
        "unharvested old fruit should cease being indefinitely available"
    );
    assert!(
        recovered,
        "retained biomass should support a later ripe crop"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn repeated_harvests_exhaust_the_same_finite_root_without_infinite_refill() {
    let (registries, mut state, id) = generated_plant();
    let initial = state.surface().get(id).unwrap().perennial_root_mass();
    let mut yielded = Mass::ZERO;
    for _ in 0..8 {
        let patch = state.surface().get(id).unwrap();
        if patch.lifecycle() != SurfaceResourceLifecycle::Available {
            break;
        }
        let mass = patch.remaining_mass();
        yielded = yielded.checked_add(mass).unwrap();
        strip_plant(&mut state, id);
        for _ in 0..4_000 {
            let _ = advance_tick(&registries, &mut state).unwrap();
            if state.surface().get(id).unwrap().lifecycle() == SurfaceResourceLifecycle::Available {
                break;
            }
        }
    }
    assert!(
        state
            .surface()
            .get(id)
            .unwrap()
            .perennial_root_mass()
            .is_zero(),
        "available biomass should be strictly finite, not created each growing season"
    );
    assert!(
        yielded <= state.surface().get(id).unwrap().initial_mass(),
        "all harvested batches together must fit the original finite plant budget"
    );
    assert!(!initial.is_zero());
    assert_eq!(
        state.surface().get(id).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn forged_perennial_biomass_and_future_crop_age_fail_trusted_load() {
    let (registries, state, id) = generated_plant();
    let initial = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let key = id.value().to_string();
    let mut extra = initial.clone();
    extra["state"]["systems"]["surface"]["resources"][&key]["perennial"]["root_reserve"] =
        serde_json::json!(u64::MAX);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(extra).unwrap();
    assert!(loaded.into_state(&registries).is_err());
    let mut future = initial;
    future["state"]["systems"]["surface"]["resources"][&key]["perennial"]["ripened_at"] =
        serde_json::to_value(SimulationTick::new(10_000)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_value(future).unwrap();
    assert!(loaded.into_state(&registries).is_err());
}

#[test]
fn removing_the_stone_footing_stops_a_depleted_shrub_from_regrowing() {
    let (registries, mut state, id) = generated_plant();
    let position = state.surface().get(id).unwrap().position();
    strip_plant(&mut state, id);
    let floor = VoxelCoord::new(position.x(), position.y() - 1, position.z());
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        floor,
        crate::world::BlockStateId::AIR,
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    for _ in 0..4_000 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.surface().get(id).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn perennial_origin_and_original_yield_are_verified_against_world_seed() {
    let (_, state, id) = generated_plant();
    let source = state.surface().get(id).unwrap();
    let plant = source.perennial.unwrap();
    let profile = state.world().generation_profile().unwrap();
    assert!(crate::world::seeded_perennial_matches(
        profile,
        source.position(),
        source.commodity(),
        plant.original_fruit,
    ));
    assert!(!crate::world::seeded_perennial_matches(
        profile,
        source.position(),
        source.commodity(),
        Mass::from_milligrams(plant.original_fruit.milligrams() + 1),
    ));
    assert!(!crate::world::seeded_perennial_matches(
        profile,
        VoxelCoord::new(
            source.position().x() + 20,
            source.position().y(),
            source.position().z()
        ),
        source.commodity(),
        plant.original_fruit,
    ));
}

/// Select an actually seeded shrub below an actual tree's future grown crown;
/// the generated district, tree and biomass are not fabricated for the test.
fn shaded_perennial() -> (
    Registries,
    AppState,
    crate::surface::SurfaceResourceId,
    VoxelCoord,
) {
    use crate::core::state::apply_clock_advance;

    let registries = build_registries();
    let calendar = registries.core().calendar();
    let season_span = calendar.ticks_per_day()
        * u64::from(calendar.days_per_month())
        * u64::from(calendar.months_per_year() / 4);
    let summer = SimulationTick::new(4 * season_span * 4 + season_span);
    for seed_value in 1..64 {
        let seed = WorldGenerationSeed::new(seed_value);
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let y = super::super::generation::terrain_height(seed, 0, 0) + 1;
        validate_admit_player(&registries, &state, VoxelCoord::new(0, y, 0))
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();
        apply_clock_advance(&mut state, summer);
        let candidate = state.surface().resources().find_map(|resource| {
            let shade = super::super::sky::living_canopy_shade_at(
                &registries,
                &state,
                resource.position(),
                summer,
            );
            if shade == 0 || resource.perennial.is_none() {
                return None;
            }
            let root = state
                .world()
                .trees_near_horizontal(resource.position(), 3)
                .find(|tree| {
                    tree.canopy_radius_voxels(summer) > 0
                        && tree
                            .root()
                            .x()
                            .abs_diff(resource.position().x())
                            .max(tree.root().z().abs_diff(resource.position().z()))
                            <= u64::from(tree.canopy_radius_voxels(summer))
                })?
                .root();
            Some((resource.id(), root))
        });
        if let Some((id, root)) = candidate {
            return (registries, state, id, root);
        }
    }
    panic!("no naturally generated canopy-shaded edible shrub in finite seed search")
}

#[test]
fn living_tree_canopy_reduces_next_real_crop_without_destroying_root_biomass() {
    let (registries, mut state, id, root) = shaded_perennial();
    let site = state.surface().get(id).unwrap().position();
    let when = state.tick();
    let shade = super::super::sky::living_canopy_shade_at(&registries, &state, site, when);
    assert!(shade > 0);
    let calendar = registries.core().calendar();
    let season_span = calendar.ticks_per_day()
        * u64::from(calendar.days_per_month())
        * u64::from(calendar.months_per_year() / 4);
    assert_eq!(
        super::super::sky::living_canopy_shade_at(
            &registries,
            &state,
            site,
            SimulationTick::new(when.value() + 2 * season_span),
        ),
        0,
        "the same deciduous crown is leafless two seasons after summer"
    );
    let original_root = state.surface().get(id).unwrap().perennial_root_mass();
    let original = state
        .surface()
        .get(id)
        .unwrap()
        .perennial
        .unwrap()
        .original_fruit;
    // Harvest the same rooted crop, then compare the next real weather-front
    // recovery with and without the mature canopy.
    strip_plant(&mut state, id);
    let mut under_tree = state.clone();
    let baseline = calculate_matter_accounting(&state).unwrap().total();
    // This is the same canonical felling operation that transfers the mature
    // tree to an ordinary recoverable surface parcel.
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(root.x() + 1, root.y(), root.z()),
    );
    let fell = super::super::felling::validate_fell_tree(&registries, &state, root).unwrap();
    let _ = fell.commit(&mut state).unwrap();
    assert!(super::super::sky::living_canopy_shade_at(&registries, &state, site, when) < shade);
    // The settled calendar projection can cross years without millions of
    // artificial player input samples. Trigger exactly the normal biological
    // owner at the next suitable 200-tick weather front in both world states.
    let profile = state.world().generation_profile().unwrap();
    // The site may be in dry scrub with a 2,400-tick recovery minimum;
    // selecting the first merely warm front would not yet permit ripening.
    let next_front = (12..60)
        .map(|step| SimulationTick::new(when.value() + step * 200))
        .find(|&at| {
            let air =
                super::super::weather::weather_at(&registries, profile.seed().value(), site, at);
            (283_150..=303_150).contains(&air.ambient_temperature().millikelvin())
        })
        .expect("a suitable warm growing front follows the harvested crop");
    crate::core::state::apply_clock_advance(&mut under_tree, next_front);
    crate::core::state::apply_clock_advance(&mut state, next_front);
    apply_natural_forage_cycle(&registries, &mut under_tree, next_front);
    apply_natural_forage_cycle(&registries, &mut state, next_front);
    let shaded = under_tree.surface().get(id).unwrap();
    let sunlit = state.surface().get(id).unwrap();
    assert_eq!(shaded.lifecycle(), SurfaceResourceLifecycle::Available);
    assert_eq!(sunlit.lifecycle(), SurfaceResourceLifecycle::Available);
    assert!(
        sunlit.remaining_mass() > shaded.remaining_mass(),
        "removing a shade-casting tree must result in a larger real edible crop"
    );
    for (record, total_root) in [(shaded, original_root), (sunlit, original_root)] {
        assert_eq!(
            record
                .remaining_mass()
                .checked_add(record.perennial_root_mass())
                .unwrap(),
            total_root,
            "each branch transfers only its own previously rooted physical matter"
        );
        assert!(record.remaining_mass() <= original);
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        baseline
    );
    assert_eq!(
        calculate_matter_accounting(&under_tree).unwrap().total(),
        baseline
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &under_tree), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, loaded);
    let another_front = SimulationTick::new(next_front.value() + 200);
    crate::core::state::apply_clock_advance(&mut state, another_front);
    crate::core::state::apply_clock_advance(&mut loaded, another_front);
    apply_natural_forage_cycle(&registries, &mut state, another_front);
    apply_natural_forage_cycle(&registries, &mut loaded, another_front);
    assert_eq!(state, loaded);
}
