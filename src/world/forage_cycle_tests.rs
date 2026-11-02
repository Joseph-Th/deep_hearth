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

#[test]
fn due_gathering_and_senescence_near_surface_revision_exhaustion_are_atomic() {
    use crate::simulation::TickError;
    use crate::surface::{
        SurfaceResourceState, player_surface_gathering_options,
        validate_start_player_surface_gathering_option,
    };

    let (registries, mut state, plant) = generated_plant();
    let position = state.surface().get(plant).unwrap().position();
    crate::logistics::relocate_player_for_test(&mut state, position);
    let batch = Mass::from_milligrams(25_000);
    let option = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.resource() == plant)
        .expect("a generated perennial must offer ordinary gathering");
    let duration = option.duration_for_mass(batch).unwrap().value();
    let due = SimulationTick::new(FRUIT_SENESCENCE_TICKS);
    assert!(duration > 0 && duration < due.value());
    crate::core::state::apply_clock_advance(
        &mut state,
        SimulationTick::new(due.value() - duration),
    );
    let option = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.resource() == plant)
        .unwrap();
    validate_start_player_surface_gathering_option(&registries, &state, option, batch)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::SurfaceGathering { work })
            if work.completes_at() == due
    ));
    while state.tick().value() < due.value() - 1 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut persisted = saved["state"]["systems"]["surface"].clone();
    persisted["revision"] = serde_json::json!(u64::MAX - 1);
    let mut near_exhausted: SurfaceResourceState = serde_json::from_value(persisted).unwrap();
    near_exhausted.rebuild_derived_indexes();
    *state.surface_state_mut() = near_exhausted;
    assert!(
        !state.can_spend_surface_revisions(1),
        "the existing gathering reservation also consumes the final surface revision"
    );

    let mut with_two_revisions = state.clone();
    let mut enough = saved["state"]["systems"]["surface"].clone();
    enough["revision"] = serde_json::json!(u64::MAX - 2);
    let mut enough_surface: SurfaceResourceState = serde_json::from_value(enough).unwrap();
    enough_surface.rebuild_derived_indexes();
    *with_two_revisions.surface_state_mut() = enough_surface;
    assert!(with_two_revisions.can_spend_surface_revisions(1));
    assert!(
        with_two_revisions
            .surface()
            .get(plant)
            .unwrap()
            .remaining_mass()
            > batch
    );
    let successful = advance_tick(&registries, &mut with_two_revisions).unwrap();
    assert_eq!(
        successful
            .surface_gathering()
            .map(|gathering| gathering.resource()),
        Some(plant)
    );
    assert_eq!(with_two_revisions.surface().revision(), u64::MAX);
    assert_eq!(
        with_two_revisions.surface().get(plant).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted,
        "the post-harvest senescence must actually spend the second revision"
    );
    assert_eq!(
        validate_loaded_state(&registries, &with_two_revisions),
        Ok(())
    );

    let before = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::SurfaceRevisionExhausted)
    );
    assert_eq!(
        state, before,
        "a completed harvest and natural senescence must reject as one atomic slow tick"
    );
}

#[test]
fn storage_enclosures_cannot_cover_living_roots_even_after_the_fruit_is_harvested() {
    use crate::content::STORAGE_ROUGH_TIMBER_FIELD_BOX;
    use crate::inventory::{
        PlayerStoragePlacementError, StorageEnclosureConstructionError,
        add_solid_stockpile_for_test, validate_build_storage_enclosure,
        validate_place_player_storage_at,
    };
    use crate::logistics::{relocate_player_for_test, validate_place_ground_stockpile};

    let (registries, mut state, plant) = generated_plant();
    let position = state.surface().get(plant).unwrap().position();
    relocate_player_for_test(&mut state, position);

    // A fruiting patch and the living roots left after harvesting must both
    // obstruct a full-cell enclosure, not merely a construction blueprint.
    for harvested in [false, true] {
        if harvested {
            strip_plant(&mut state, plant);
            assert_eq!(
                state.surface().get(plant).unwrap().lifecycle(),
                SurfaceResourceLifecycle::Depleted
            );
        }
        let before = state.clone();
        assert_eq!(
            validate_place_player_storage_at(
                &registries,
                &state,
                STORAGE_ROUGH_TIMBER_FIELD_BOX,
                position,
            )
            .err(),
            Some(PlayerStoragePlacementError::RootedVegetationOccupiesSite { position })
        );
        assert_eq!(state, before);
    }

    // Enclosing an existing loose ground stockpile is a separate, public
    // construction path and must enforce the identical root-footprint rule.
    let stockpile =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    validate_place_ground_stockpile(&state, stockpile, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_ROUGH_TIMBER_FIELD_BOX,
            stockpile,
            stockpile,
        )
        .err(),
        Some(StorageEnclosureConstructionError::RootedVegetationOccupiesSite { position })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state,
    );
}

#[test]
fn trusted_load_rejects_enclosures_relocated_onto_rooted_food() {
    use crate::content::{FORM_ROUGH_BOX_BODY, MATERIAL_WOOD, STORAGE_ROUGH_TIMBER_FIELD_BOX};
    use crate::core::state::StateValidationError;
    use crate::inventory::{deposit_bulk_for_test, validate_place_player_storage};
    use crate::material::CommodityKey;
    use crate::persistence::LoadError;

    let (registries, mut state, plant) = generated_plant();
    let root = state.surface().get(plant).unwrap().position();
    let spawn = state.logistics().player().unwrap().position();
    assert_ne!(
        spawn, root,
        "the generated opening must have distinct forage and camp sites"
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        Mass::from_milligrams(1_600_000),
    )
    .unwrap();
    let enclosure =
        validate_place_player_storage(&registries, &state, STORAGE_ROUGH_TIMBER_FIELD_BOX)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // Alter only persisted location custody, as a malformed save might. The
    // derived spatial index is rebuilt during promotion, never trusted blindly.
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["logistics"]["stockpile_locations"]
        [enclosure.value().to_string()] =
        serde_json::json!({"x": root.x(), "y": root.y(), "z": root.z()});
    let decoded = serde_json::from_value::<LoadedSaveEnvelope>(encoded).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::EnclosureOverlapsRootedForage { position: root }
        ))
    );
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
fn depleted_living_roots_cannot_be_buried_in_loaded_terrain() {
    use crate::core::state::StateValidationError;
    use crate::surface::SurfaceResourceValidationError;

    let (registries, mut state, plant) = generated_plant();
    let position = state.surface().get(plant).unwrap().position();
    strip_plant(&mut state, plant);
    assert!(
        state
            .surface()
            .get(plant)
            .unwrap()
            .rooted_biomass_occupies_site()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        position,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(StateValidationError::Surface(
            SurfaceResourceValidationError::EmbeddedInSolidWorld {
                resource: plant,
                voxel: position,
            }
        ))
    );
}

#[test]
fn full_footprint_construction_cannot_embody_member_over_harvested_roots() {
    use crate::content::{
        BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS, FORM_LOG, MATERIAL_WOOD,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    };
    use crate::core::quantity::Temperature;
    use crate::inventory::{MaterialLotSelection, deposit_lot_for_test};
    use crate::logistics::relocate_player_for_test;
    use crate::material::CommodityKey;
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        GroundedStructuralElementPlan, PlayerStructuralConstructionError,
        StructuralConstructionError, StructuralElementGeometry,
        validate_plan_player_grounded_structural_element,
        validate_start_player_structural_construction,
    };

    let (registries, mut state, plant) = generated_plant();
    let root = state.surface().get(plant).unwrap().position();
    strip_plant(&mut state, plant);
    let neighbor = [(-1, 0), (1, 0), (0, -1), (0, 1)]
        .into_iter()
        .map(|(dx, dz)| VoxelCoord::new(root.x() + dx, root.y(), root.z() + dz))
        .find(|position| {
            state
                .world()
                .has_standing_clearance(registries.world(), *position)
        })
        .expect("seeded forage has a traversable neighboring construction position");
    relocate_player_for_test(&mut state, neighbor);
    let bounds = VoxelBounds::new(
        root,
        VoxelCoord::new(root.x() + 1, root.y() + 1, root.z() + 1),
    )
    .unwrap();
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
        BASIC_TIMBER_COVER_PANEL_AREA,
    )
    .unwrap();
    assert!(geometry.covers_horizontal_footprint());
    let element = validate_plan_player_grounded_structural_element(
        &registries,
        &state,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let mass = Mass::from_milligrams(6_500_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_start_player_structural_construction(
            &registries,
            &state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            element,
            &[MaterialLotSelection::new(lot, mass)],
        )
        .err(),
        Some(PlayerStructuralConstructionError::Material(
            StructuralConstructionError::RootedVegetationOccupied {
                element,
                voxel: root,
            }
        ))
    );
    assert_eq!(
        state, before,
        "rejected materialization cannot consume timber"
    );
}

#[test]
fn spent_perennial_provenance_does_not_permanently_reserve_building_land() {
    let (_, mut state, plant) = generated_plant();
    let mut fruiting = state.surface().get(plant).unwrap().clone();
    assert!(fruiting.rooted_biomass_occupies_site());
    fruiting.perennial.as_mut().unwrap().root_reserve = Mass::ZERO;
    assert!(
        fruiting.rooted_biomass_occupies_site(),
        "the last unharvested fruit still has physical mass at the site"
    );
    strip_plant(&mut state, plant);
    let mut stripped = state.surface().get(plant).unwrap().clone();
    assert!(stripped.rooted_biomass_occupies_site());
    assert_eq!(stripped.remaining_mass(), Mass::ZERO);
    stripped.perennial.as_mut().unwrap().root_reserve = Mass::ZERO;
    assert!(
        !stripped.rooted_biomass_occupies_site(),
        "an exhausted, fruitless provenance record is not living vegetation"
    );
}

#[test]
fn canopy_darkness_and_sub_milligram_yields_leave_root_matter_intact() {
    let roots = Mass::from_milligrams(800);
    let fruit = Mass::from_milligrams(800);
    assert_eq!(
        ripened_crop(roots, fruit, RootMoisture::NoBasin, 8),
        Mass::from_milligrams(48),
        "midday skylight still reaches a canopy with shade eight"
    );
    assert_eq!(
        ripened_crop(roots, fruit, RootMoisture::NoBasin, 14),
        Mass::ZERO
    );
    assert_eq!(
        ripened_crop(
            Mass::from_milligrams(1),
            Mass::from_milligrams(1),
            RootMoisture::NoBasin,
            6
        ),
        Mass::ZERO,
        "fractional milligrams cannot be rounded into edible food"
    );
    assert_eq!(
        ripened_crop(roots, fruit, RootMoisture::NoBasin, 6),
        Mass::from_milligrams(75)
    );
    assert_eq!(
        ripened_crop(roots, fruit, RootMoisture::NoBasin, 0),
        Mass::from_milligrams(200)
    );
    assert_eq!(
        ripened_crop(roots, fruit, RootMoisture::NoBasin, 0)
            .checked_add(Mass::from_milligrams(600)),
        Some(roots)
    );
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
    apply_natural_forage_cycle(&registries, &mut under_tree, next_front).unwrap();
    apply_natural_forage_cycle(&registries, &mut state, next_front).unwrap();
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
    apply_natural_forage_cycle(&registries, &mut state, another_front).unwrap();
    apply_natural_forage_cycle(&registries, &mut loaded, another_front).unwrap();
    assert_eq!(state, loaded);
}

#[test]
fn natural_senescence_does_not_spend_a_later_gathering_completion_revision() {
    use crate::content::{FORM_LUMP, MATERIAL_STONE, SURFACE_GATHERING_HAND_COLLECT_STONE};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::surface::{
        SurfaceResourceSpec, SurfaceResourceState, validate_create_surface_resources,
        validate_start_player_surface_gathering,
    };

    let (registries, mut state, plant) = generated_plant();
    let player_position = state.logistics().player().unwrap().position();
    let stone = crate::surface::SurfaceResourceId::new(state.surface().next_resource_id());
    let stone_spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        player_position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        crate::core::quantity::Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [stone_spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let next_front = SimulationTick::new(FRUIT_SENESCENCE_TICKS);
    crate::core::state::apply_clock_advance(
        &mut state,
        SimulationTick::new(next_front.value() - 1),
    );
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let gathering_due = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("expected active gathering: {other:?}"),
    };
    assert!(gathering_due > next_front);

    let mut unreserved = state.clone();
    let _ = advance_tick(&registries, &mut unreserved).unwrap();
    assert_eq!(
        unreserved.surface().get(plant).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted,
        "the same front must actually trigger natural senescence"
    );

    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut surface = saved["state"]["systems"]["surface"].clone();
    surface["revision"] = serde_json::json!(u64::MAX - 1);
    let mut near_exhausted: SurfaceResourceState = serde_json::from_value(surface).unwrap();
    near_exhausted.rebuild_derived_indexes();
    *state.surface_state_mut() = near_exhausted;
    assert!(!state.can_spend_surface_revisions(1));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(state.surface().revision(), u64::MAX - 1);
    assert_eq!(
        state.surface().get(plant).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Available,
        "incidental senescence must defer instead of spending reserved revision space"
    );
    while state.tick() < gathering_due {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.player_work().active(), None);
    assert_eq!(state.surface().revision(), u64::MAX);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn exhausted_perennial_revision_rejects_due_senescence_atomically() {
    use crate::simulation::TickError;
    use crate::surface::SurfaceResourceState;

    let (registries, mut state, plant) = generated_plant();
    let due = SimulationTick::new(FRUIT_SENESCENCE_TICKS);
    crate::core::state::apply_clock_advance(&mut state, SimulationTick::new(due.value() - 1));
    assert_eq!(
        state.surface().get(plant).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Available
    );

    let save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut persisted = save["state"]["systems"]["surface"].clone();
    persisted["revision"] = serde_json::json!(u64::MAX);
    let mut exhausted: SurfaceResourceState = serde_json::from_value(persisted).unwrap();
    exhausted.rebuild_derived_indexes();
    *state.surface_state_mut() = exhausted;

    let before = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::SurfaceRevisionExhausted)
    );
    assert_eq!(
        state, before,
        "the late ripening decision cannot partially settle a tick"
    );
}
