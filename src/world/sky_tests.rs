//! Daylight uses actual generated geometry and the authoritative calendar.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::world::{
    BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
    validate_generate_chunk,
};

#[test]
fn generated_sky_has_morning_afternoon_evening_and_night_but_no_light_in_old_graph() {
    let registries = build_registries();
    let mut state = AppState::new();
    let coord = VoxelCoord::new(2, 8, 2);
    assert_eq!(assess_generated_skylight(&registries, &state, coord), 0);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x534B_594C_4947_4854),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = (-20..=8)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(2, y, 2)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let outside = VoxelCoord::new(2, floor + 1, 2);
    assert_eq!(state.world().block_at(outside), Some(BlockStateId::AIR));
    let day_ticks = registries.core().calendar().ticks_per_day();
    for (tick, expected) in [
        (0, 9_u8),
        (day_ticks / 4, 14),
        (day_ticks / 2, 6),
        (day_ticks * 3 / 4, 0),
    ] {
        apply_clock_advance(&mut state, SimulationTick::new(tick));
        assert_eq!(
            assess_generated_skylight(&registries, &state, outside),
            expected
        );
        assert_eq!(
            assess_local_illumination(&registries, &state, outside).level(),
            0
        );
        assert_eq!(
            assess_local_illumination(&registries, &state, outside).strongest_source(),
            None
        );
    }
    let before = state.clone();
    assert_eq!(assess_generated_skylight(&registries, &state, outside), 0);
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(assess_generated_skylight(&registries, &loaded, outside), 0);
}

/// Generated-world vision depends on ambient daylight or genuine artificial
/// illumination, not the existence of a known source in hidden state.
#[test]
fn night_hides_real_surface_food_from_player_sight_without_erasing_custody() {
    use crate::content::{
        CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_FOOD, FORM_LOG, FORM_LUMP,
        MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::energy::{validate_assemble_energy_store, validate_combust_fuel};
    use crate::inventory::deposit_bulk_for_test;
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::player::validate_admit_player;
    use crate::surface::{
        AdjacentSurfaceResource, SurfaceResourceKind, SurfaceResourceSpec,
        validate_create_surface_resources,
    };
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x534B_594C_4947_4854),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = (-20..=8)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(2, y, 2)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let patch = VoxelCoord::new(2, floor + 1, 2);
    let carried = validate_admit_player(&registries, &state, patch)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let source = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        patch,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(400_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &state, [source]).unwrap();
    let source_id = plan.resource_ids().next().unwrap();
    plan.commit(&mut state).unwrap();
    let visible = |state: &AppState| {
        AdjacentSurfaceResource::visible_in_generated_world(&registries, state)
            .any(|sighting| sighting.resource() == source_id)
    };
    assert!(visible(&state));
    let day_ticks = registries.core().calendar().ticks_per_day();
    apply_clock_advance(&mut state, SimulationTick::new(3 * day_ticks / 4));
    assert_eq!(assess_generated_skylight(&registries, &state, patch), 0);
    assert!(
        !visible(&state),
        "unlit night must hide even a previously observed patch"
    );
    assert_eq!(
        state.surface().get(source_id).unwrap().remaining_mass(),
        Mass::from_milligrams(400_000)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert!(!visible(&loaded));

    // A legitimately fueled campfire gives the player back local sight at night.
    let mut lit = loaded.clone();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            CAMPFIRE_FUEL_MASS,
        ),
    ] {
        deposit_bulk_for_test(&registries, &mut lit, carried, commodity, mass).unwrap();
    }
    let fire =
        validate_assemble_energy_store(&registries, &lit, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut lit)
            .unwrap();
    validate_combust_fuel(&registries, &lit, fire, carried)
        .unwrap()
        .commit(&mut lit)
        .unwrap();
    assert_eq!(assess_generated_skylight(&registries, &lit, patch), 0);
    assert!(assess_local_illumination(&registries, &lit, patch).level() > 0);
    assert!(
        visible(&lit),
        "real finite campfire light must restore sight in darkness"
    );
    assert_eq!(validate_loaded_state(&registries, &lit), Ok(()));

    apply_clock_advance(&mut state, SimulationTick::new(day_ticks));
    assert!(
        visible(&state),
        "daybreak should reveal the same persisted food"
    );
}

/// A real seed-rooted deciduous tree changes the understory as atmospheric
/// carbon becomes wood. It casts no solid voxel, so the same food can still
/// be seen, but the light read must track crown size and calendar season.
#[test]
fn seed_rooted_tree_growth_and_season_change_real_canopy_daylight() {
    use crate::world::WorldGenerationProfile;
    let registries = build_registries();
    let calendar = registries.core().calendar();
    let days_per_season =
        u64::from(calendar.days_per_month()) * u64::from(calendar.months_per_year() / 4);
    let season_ticks = calendar.ticks_per_day() * days_per_season;
    let year_ticks = season_ticks * 4;
    let mut observed = None;
    for seed_value in 1..300 {
        let seed = WorldGenerationSeed::new(seed_value);
        let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
        let Some(origin) =
            super::super::trees::seeded_tree_identity(profile, ChunkCoord::new(0, 0))
        else {
            continue;
        };
        if origin.canopy_radius_voxels(SimulationTick::ZERO) != 0
            || origin.canopy_radius_voxels(SimulationTick::new(year_ticks * 48 + season_ticks)) < 2
        {
            continue;
        }
        let root = origin.root();
        let position = VoxelCoord::new(root.x() + 1, root.y(), root.z());
        if super::super::generation::terrain_height(seed, position.x(), position.z())
            >= position.y()
        {
            continue;
        }
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
        if state.world().block_at(position) != Some(BlockStateId::AIR) {
            continue;
        }
        observed = Some((state, root, position));
        break;
    }
    let (mut state, root, position) = observed
        .expect("a finite seed search provides a juvenile tree with a clear understory cell");
    let tree = *state.world().tree_at(root).unwrap();
    assert_eq!(tree.canopy_radius_voxels(SimulationTick::ZERO), 0);
    apply_clock_advance(&mut state, SimulationTick::new(season_ticks));
    assert_eq!(calendar.date_at(state.tick()).season(), Season::Summer);
    let juvenile_summer = assess_generated_skylight(&registries, &state, position);
    assert_eq!(juvenile_summer, 9);
    let after_four_years = year_ticks * 4 + season_ticks;
    apply_clock_advance(&mut state, SimulationTick::new(after_four_years));
    assert_eq!(calendar.date_at(state.tick()).season(), Season::Summer);
    let sapling_radius = tree.canopy_radius_voxels(state.tick());
    assert!(sapling_radius >= 1);
    let sapling_summer = assess_generated_skylight(&registries, &state, position);
    assert!(sapling_summer < juvenile_summer);
    assert_eq!(sapling_summer, 9 - sapling_radius * 2);

    let after_forty_eight_years = year_ticks * 48 + season_ticks;
    apply_clock_advance(&mut state, SimulationTick::new(after_forty_eight_years));
    let grown_radius = tree.canopy_radius_voxels(state.tick());
    assert!(
        grown_radius >= 2,
        "time alone enlarges the same rooted crown"
    );
    let grown_summer = assess_generated_skylight(&registries, &state, position);
    assert_eq!(grown_summer, 9 - grown_radius * 2);
    assert!(grown_summer < sapling_summer);

    // With the same mature tree still rooted, leafless winter admits the
    // full open-column light. Neither transition moves or creates wood lots.
    let winter = year_ticks * 48 + season_ticks * 3;
    apply_clock_advance(&mut state, SimulationTick::new(winter));
    assert_eq!(calendar.date_at(state.tick()).season(), Season::Winter);
    assert_eq!(assess_generated_skylight(&registries, &state, position), 9);
    assert_eq!(state.world().tree_at(root).copied(), Some(tree));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        assess_generated_skylight(&registries, &restored, position),
        9
    );
}

#[test]
fn physical_roof_and_tree_trunk_block_daylight_but_not_adjacent_outdoors() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x534B_594C_4947_4854),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let choose = (3..13)
        .flat_map(|x| (3..13).map(move |z| (x, z)))
        .find_map(|(x, z)| {
            let floor = (-20..=8).rev().find(|&y| {
                state.world().block_at(VoxelCoord::new(x, y, z)) == Some(BLOCK_STATE_STONE)
            })?;
            let at = VoxelCoord::new(x, floor + 1, z);
            (state.world().block_at(at) == Some(BlockStateId::AIR)
                && state.world().tree_trunk_at(at).is_none()
                && state
                    .world()
                    .tree_trunk_at(VoxelCoord::new(x, floor + 2, z))
                    .is_none()
                && state.world().block_at(VoxelCoord::new(x, floor + 2, z))
                    == Some(BlockStateId::AIR))
            .then_some(at)
        })
        .unwrap();
    assert_eq!(assess_generated_skylight(&registries, &state, choose), 9);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(choose.x(), choose.y() + 2, choose.z()),
        BLOCK_STATE_STONE,
    );
    assert_eq!(assess_generated_skylight(&registries, &state, choose), 0);
    assert_eq!(
        assess_local_illumination(&registries, &state, choose).level(),
        0
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
