//! Process output reservations must not turn empty location markers into buried matter.

use super::*;
use crate::content::BLOCK_STATE_STONE;
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
};

fn powered_heating_fixture() -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    ResolvedSensibleHeating,
) {
    use crate::content::{
        ENERGY_ELECTRICAL_BUFFER, EQUIPMENT_ELECTRIC_FURNACE, PROCESS_HEAT_MATERIAL_BATCH,
        build_registries,
    };
    use crate::equipment::validate_mount_equipment;

    let registries = build_registries();
    let mut state = AppState::new();
    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let mass = Mass::from_milligrams(10_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let furnace = add_equipment(
        &registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap();
    let support = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            VoxelBounds::new(VoxelCoord::new(100, 100, 0), VoxelCoord::new(101, 101, 1)).unwrap(),
            Length::from_micrometers(1),
            Area::from_square_millimeters(500_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, support, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_mount_equipment(&registries, &state, furnace, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let power = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::from_nanojoules(2_000_000_000_000_000),
    )
    .unwrap();
    let resolution = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS_HEAT_MATERIAL_BATCH,
            source,
            &[MaterialLotSelection::new(lot, mass)],
            furnace,
            power,
            Temperature::from_millikelvin(303_150),
        ),
    )
    .unwrap();
    (registries, state, source, destination, resolution)
}

fn player_heating_fixture() -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    ResolvedSensibleHeating,
) {
    use crate::logistics::{place_energy_store_for_fixture, place_equipment_for_fixture};

    let (registries, mut state, source, _, resolution) = powered_heating_fixture();
    let at = VoxelCoord::new(100, 100, 0);
    validate_place_ground_stockpile(&state, source, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    place_equipment_for_fixture(&mut state, resolution.equipment(), at);
    place_energy_store_for_fixture(
        &mut state,
        resolution
            .process_resolution()
            .energy_input()
            .unwrap()
            .source(),
        at,
    );
    let carried = validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == source)
        .unwrap();
    let resolution = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            crate::content::PROCESS_HEAT_MATERIAL_BATCH,
            source,
            &[MaterialLotSelection::new(lot.id(), lot.mass())],
            resolution.equipment(),
            resolution
                .process_resolution()
                .energy_input()
                .unwrap()
                .source(),
            resolution.target(),
        ),
    )
    .unwrap();
    (registries, state, source, carried, resolution)
}

#[test]
fn terrain_generation_cannot_drop_carried_output_away_from_retained_machine() {
    let (registries, mut state, source, carried, resolution) = player_heating_fixture();
    let chunk = ChunkCoord::new(6, 0);
    let job = validate_start_player_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        carried,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    let generate = |state: &AppState| {
        validate_generate_chunk(
            &registries,
            state,
            WorldGenerationSeed::new(41),
            chunk,
            BLOCK_STATE_STONE,
        )
    };
    assert_eq!(
        generate(&state).err(),
        Some(
            crate::world::ChunkGenerationError::RetainedProductionSiteAtGenerationSite {
                chunk,
                job
            }
        )
    );
    assert_eq!(state, before);
    while state.production().get_job(job).is_some() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(generate(&state).is_ok(), "completed work releases the site");
}

#[test]
fn retained_carried_output_requires_the_exact_player_floor() {
    use crate::spatial::{NANOMETERS_PER_VOXEL, WorldPoint};
    use crate::world::{BlockStateId, set_generated_block_for_test};
    let (registries, mut state, source, carried, resolution) = player_heating_fixture();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(6, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let at = state.logistics().player().unwrap().position();
    let floor = VoxelCoord::new(at.x(), at.y() - 1, at.z());
    let ledge = VoxelCoord::new(at.x() + 1, floor.y(), floor.z());
    for voxel in [floor, ledge] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BLOCK_STATE_STONE);
    }
    let player = state.logistics().player().copied().unwrap();
    let body = player.body();
    let shifted = body.with_motion(
        WorldPoint::new(
            at.x() * NANOMETERS_PER_VOXEL + 950_000_000,
            body.foot_position().y_nanometers(),
            body.foot_position().z_nanometers(),
        ),
        body.horizontal_velocity(),
        body.vertical_velocity(),
        body.stance(),
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_player_body_motion(
        revision,
        revision + 1,
        body,
        shifted,
        player.locomotion_exertion(),
        player.pending_fall_vitality_loss_ppm(),
    );
    let start = |state: &AppState| {
        validate_start_player_process(
            &registries,
            state,
            resolution.process_resolution(),
            source,
            carried,
        )
    };
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let mut admitted = state.clone();
    let job = start(&admitted).unwrap().commit(&mut admitted).unwrap();
    assert_eq!(validate_loaded_state(&registries, &admitted), Ok(()));
    set_generated_block_for_test(registries.world(), &mut admitted, floor, BlockStateId::AIR);
    assert_eq!(
        validate_loaded_state(&registries, &admitted),
        Err(StateValidationError::JobRetainedPlayerSiteRequiresFixedFloor { job, position: at })
    );
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    assert!(crate::logistics::player_body_has_fixed_floor(
        &registries,
        &state
    ));
    assert!(!crate::logistics::player_stationary_placement_supported(
        &registries,
        &state
    ));
    let before = state.clone();
    assert_eq!(
        start(&state).err(),
        Some(StartPlayerProcessError::Process(
            StartProcessError::RetainedPlayerSiteRequiresFixedFloor { position: at }
        ))
    );
    assert_eq!(state, before);
}

fn rock_location() -> VoxelCoord {
    VoxelCoord::new(5, WORLD_MIN_Y, 5)
}

fn generate_rock(registries: &Registries, state: &mut AppState) {
    validate_generate_chunk(
        registries,
        state,
        WorldGenerationSeed::new(0x5052_4F43_524F_434B),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(state)
    .unwrap();
    assert_eq!(
        state.world().block_at(rock_location()),
        Some(BLOCK_STATE_STONE)
    );
}

#[test]
fn reserved_production_output_prevents_later_generation_from_burying_matter() {
    use crate::world::ChunkGenerationError;

    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    validate_place_ground_stockpile(&state, destination, rock_location())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let planned = validate_start_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        destination,
    )
    .unwrap();
    planned.commit(&mut state).unwrap();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap()
            .reserved_inbound(),
        Mass::from_milligrams(10_000)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            WorldGenerationSeed::new(0x5052_4F43_524F_434B),
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::GroundMatterBuriedByTerrain {
            voxel: rock_location(),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn supported_production_output_to_generated_air_remains_valid_and_round_trips() {
    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    generate_rock(&registries, &mut state);
    let at = ground_location(&state);
    assert_eq!(
        state.world().block_at(at),
        Some(crate::world::BlockStateId::AIR)
    );
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_start_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );
}

fn ground_location(state: &AppState) -> VoxelCoord {
    let floor_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    VoxelCoord::new(5, floor_y + 1, 5)
}

#[test]
fn production_cannot_reserve_output_into_unsupported_generated_air() {
    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    generate_rock(&registries, &mut state);
    let at = VoxelCoord::new(5, 100, 5);
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            resolution.process_resolution(),
            source,
            destination
        )
        .err(),
        Some(StartProcessError::OutputDestinationRequiresFixedFloor {
            stockpile: destination,
            position: at
        })
    );
    assert_eq!(state, before);
}

#[test]
fn reserved_production_output_prevents_generation_without_footing() {
    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    let at = VoxelCoord::new(5, 100, 5);
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_start_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            WorldGenerationSeed::new(0x5052_4F43_524F_434B),
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(crate::world::ChunkGenerationError::ReservedOutputRequiresFixedFloor { voxel: at })
    );
    assert_eq!(state, before);
}

#[test]
fn production_output_footing_is_rechecked_at_commit_and_trusted_load() {
    use crate::world::{BlockStateId, set_generated_block_for_test};
    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    generate_rock(&registries, &mut state);
    let at = ground_location(&state);
    let floor = VoxelCoord::new(at.x(), at.y() - 1, at.z());
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let plan = |state: &AppState| {
        validate_start_process(
            &registries,
            state,
            resolution.process_resolution(),
            source,
            destination,
        )
        .unwrap()
    };
    let admitted = plan(&state);
    let mut forged = state.clone();
    plan(&forged).commit(&mut forged).unwrap();
    let expected = state.world().revision();
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    let actual = state.world().revision();
    let before = state.clone();
    assert_eq!(
        admitted.commit(&mut state),
        Err(StartProcessCommitError::StaleWorldRevision { expected, actual })
    );
    assert_eq!(state, before);
    set_generated_block_for_test(registries.world(), &mut forged, floor, BlockStateId::AIR);
    assert_eq!(
        validate_loaded_state(&registries, &forged),
        Err(crate::core::state::StateValidationError::Logistics(
            crate::logistics::LogisticsValidationError::ReservedGroundMatterRequiresFixedFloor {
                stockpile: destination,
                voxel: at,
            }
        ))
    );
}

#[test]
fn suspended_output_cannot_be_unmounted_without_ground_footing() {
    use crate::inventory::{
        StockpileSupportError, validate_mount_stockpile, validate_unmount_stockpile,
    };
    use crate::world::{BlockStateId, set_generated_block_for_test};
    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    generate_rock(&registries, &mut state);
    let at = ground_location(&state);
    let floor = VoxelCoord::new(at.x(), at.y() - 1, at.z());
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let support = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            VoxelBounds::new(at, VoxelCoord::new(at.x() + 1, at.y() + 1, at.z() + 1)).unwrap(),
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, support, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == source)
        .unwrap();
    let resolution = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            crate::content::PROCESS_HEAT_MATERIAL_BATCH,
            source,
            &[MaterialLotSelection::new(lot.id(), lot.mass())],
            resolution.equipment(),
            resolution
                .process_resolution()
                .energy_input()
                .unwrap()
                .source(),
            resolution.target(),
        ),
    )
    .unwrap();
    let job = validate_start_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let _ = advance_tick(&registries, &mut state).unwrap();
    assert!(
        state
            .production()
            .get_job(job)
            .unwrap()
            .suspension()
            .is_some()
    );
    assert!(validate_unmount_stockpile(&registries, &state, destination).is_ok());
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    let before = state.clone();
    assert_eq!(
        validate_unmount_stockpile(&registries, &state, destination).err(),
        Some(StockpileSupportError::UnmountedMatterRequiresFixedFloor {
            stockpile: destination,
            position: at
        })
    );
    assert_eq!(state, before);
    set_generated_block_for_test(registries.world(), &mut state, floor, BLOCK_STATE_STONE);
    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn production_cannot_reserve_output_into_empty_ground_location_inside_rock() {
    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    generate_rock(&registries, &mut state);
    validate_place_ground_stockpile(&state, destination, rock_location())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            resolution.process_resolution(),
            source,
            destination
        )
        .err(),
        Some(StartProcessError::OutputDestinationSolidTerrain {
            stockpile: destination,
            position: rock_location(),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn production_start_rechecks_terrain_if_chunk_generated_after_admission() {
    let (registries, mut state, source, destination, resolution) = powered_heating_fixture();
    validate_place_ground_stockpile(&state, destination, rock_location())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let admitted = validate_start_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        destination,
    )
    .expect("an ungenerated ground marker can be reserved before physical terrain exists");
    generate_rock(&registries, &mut state);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        admitted.commit(&mut state),
        Err(StartProcessCommitError::OutputDestinationSolidTerrain {
            stockpile: destination,
            position: rock_location(),
        })
    );
    assert_eq!(state, before);
}
