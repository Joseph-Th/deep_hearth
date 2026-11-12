//! Physical loose custody must not remain suspended when its stone floor is mined.

use super::*;
use crate::inventory::deposit_lot_for_test;
use crate::logistics::validate_place_ground_stockpile;

fn nearly_mined_floor() -> (Registries, AppState, EquipmentId, VoxelCoord, VoxelCoord) {
    let (registries, mut state, pick, floor) = generated_excavation_fixture();
    let original = state
        .world()
        .represented_mass_at(registries.world(), floor)
        .unwrap();
    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        floor,
        BLOCK_STATE_STONE,
        original,
        Mass::from_milligrams(2),
    );
    let above = VoxelCoord::new(floor.x(), floor.y() + 1, floor.z());
    (registries, state, pick, floor, above)
}

fn final_cut(pick: EquipmentId) -> BlockExcavationRequest {
    BlockExcavationRequest::new(
        positive_x_view(),
        MINING_METHOD_HAND_PICK,
        pick,
        Mass::from_milligrams(2),
    )
}

#[test]
fn loose_surface_parcel_cannot_be_left_floating_by_full_excavation() {
    let (registries, mut state, pick, floor, above) = nearly_mined_floor();
    let pending = validate_start_player_block_excavation(&registries, &state, final_cut(pick))
        .expect("floor without a parcel should be mineable");
    let specification = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        above,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        crate::material::MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [specification]).unwrap();
    let parcel = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        pending.commit(&mut state),
        Err(BlockExcavationCommitError::LooseSurfaceDropRequiresFloor {
            target: floor,
            resource: parcel
        })
    );
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, final_cut(pick)).err(),
        Some(BlockExcavationError::LooseSurfaceDropRequiresFloor {
            target: floor,
            resource: parcel
        })
    );
    assert_eq!(state, before);
    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(positive_x_view())),
    )
    .unwrap();
    let started = match control.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(started)) => *started,
        other => panic!("held excavation should safely retain the last milligram: {other:?}"),
    };
    assert_eq!(started.excavated_mass(), Mass::from_milligrams(1));
    while state.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.world().represented_mass_at(registries.world(), floor),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(state.surface().get(parcel).unwrap().position(), above);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );
}

#[test]
fn occupied_unenclosed_ground_cache_cannot_be_left_floating_by_full_excavation() {
    let (registries, mut state, pick, floor, above) = nearly_mined_floor();
    let cache = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500_000)).unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        cache,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_ground_stockpile(&state, cache, above)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, final_cut(pick)).err(),
        Some(BlockExcavationError::OccupiedGroundStockpileRequiresFloor {
            target: floor,
            stockpile: cache
        })
    );
    assert_eq!(state, before);
    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(positive_x_view())),
    )
    .unwrap();
    let started = match control.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(started)) => *started,
        other => panic!("held excavation must spare the occupied cache: {other:?}"),
    };
    assert_eq!(started.excavated_mass(), Mass::from_milligrams(1));
    while state.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.world().represented_mass_at(registries.world(), floor),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(
        state.logistics().stationary_stockpile_position(cache),
        Some(above)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn empty_ground_marker_does_not_block_complete_excavation() {
    let (registries, mut state, pick, floor, above) = nearly_mined_floor();
    let empty = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000)).unwrap();
    validate_place_ground_stockpile(&state, empty, above)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let started = validate_start_player_block_excavation(&registries, &state, final_cut(pick))
        .expect("a location marker with no matter does not claim footing")
        .commit(&mut state)
        .unwrap();
    while state.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.world().block_at(floor), Some(BlockStateId::AIR));
    assert_eq!(
        state.logistics().stationary_stockpile_position(empty),
        Some(above)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn actual_player_drop_protects_its_floor_from_held_excavation() {
    use crate::surface::validate_drop_player_material;

    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let position = state.logistics().player().unwrap().position();
    let floor = VoxelCoord::new(position.x(), position.y() - 1, position.z());
    let full = state
        .world()
        .represented_mass_at(registries.world(), floor)
        .unwrap();
    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        floor,
        BLOCK_STATE_STONE,
        full,
        Mass::from_milligrams(2),
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let dropped = validate_drop_player_material(&registries, &state, commodity)
        .expect("player on solid generated footing can leave conserved loose matter")
        .commit(&mut state)
        .unwrap();
    assert_eq!(dropped.dropped_mass(), Mass::from_milligrams(100_000));
    let parcel = state
        .surface()
        .available_resources_at(Some(position))
        .next()
        .unwrap()
        .id();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let down = ViewDirection::new(0, -VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap();
    assert_eq!(
        validate_start_player_block_excavation(
            &registries,
            &state,
            BlockExcavationRequest::new(
                down,
                MINING_METHOD_HAND_PICK,
                pick,
                Mass::from_milligrams(2)
            ),
        )
        .err(),
        Some(BlockExcavationError::LooseSurfaceDropRequiresFloor {
            target: floor,
            resource: parcel
        })
    );
    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::break_target(down)),
    )
    .unwrap();
    let started = match control.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(started)) => *started,
        other => panic!("held pick must allow safe partial mining: {other:?}"),
    };
    assert_eq!(started.excavated_mass(), Mass::from_milligrams(1));
    while state.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.world().represented_mass_at(registries.world(), floor),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn depleted_perennial_roots_still_need_stone_footing() {
    use crate::logistics::relocate_player_for_test;
    use crate::surface::SurfaceResourceLifecycle;

    // This known wilderness seed supplies genuine seed-derived perennial
    // provenance. A fabricated plant correctly fails trusted-save admission.
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x464F_5241_4745_434F);
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
    let spawn = VoxelCoord::new(0, super::generation::terrain_height(seed, 0, 0) + 1, 0);
    let pick = assemble_stone_pick_at(&registries, &mut state, spawn);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("the neighboring chunks must be generated")
        .commit(&mut state)
        .unwrap();
    let record = state
        .surface()
        .perennial_resources()
        .next()
        .expect("generated neighborhood must contain a seed-derived perennial");
    let root = record.id();
    let above = record.position();
    let floor = VoxelCoord::new(above.x(), above.y() - 1, above.z());
    let full_mass = state
        .world()
        .represented_mass_at(registries.world(), floor)
        .unwrap();
    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        floor,
        BLOCK_STATE_STONE,
        full_mass,
        Mass::from_milligrams(2),
    );
    relocate_player_for_test(&mut state, above);
    let record = state.surface().get(root).unwrap();
    let mut perennial = record.perennial.unwrap();
    perennial.root_reserve = perennial
        .root_reserve
        .checked_add(record.remaining_mass())
        .unwrap();
    let next_revision = state.surface().revision() + 1;
    state.surface_state_mut().apply_perennial_cycle(
        vec![(
            root,
            perennial,
            Mass::ZERO,
            SurfaceResourceLifecycle::Depleted,
            Some(SimulationTick::ZERO),
        )],
        next_revision,
    );
    assert!(state.surface().rooted_biomass_occupies_voxel(above));
    assert!(
        state
            .surface()
            .available_resources_at(Some(above))
            .next()
            .is_none()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    let down = ViewDirection::new(0, -VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap();
    assert_eq!(
        validate_start_player_block_excavation(
            &registries,
            &state,
            BlockExcavationRequest::new(
                down,
                MINING_METHOD_HAND_PICK,
                pick,
                Mass::from_milligrams(2)
            ),
        )
        .err(),
        Some(BlockExcavationError::RootedForageRequiresFloor { target: floor })
    );
    assert_eq!(state, before);
    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::break_target(down)),
    )
    .unwrap();
    let started = match control.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(started)) => *started,
        other => panic!("held pick should spare the living root: {other:?}"),
    };
    assert_eq!(started.excavated_mass(), Mass::from_milligrams(1));
    while state.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.world().represented_mass_at(registries.world(), floor),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
