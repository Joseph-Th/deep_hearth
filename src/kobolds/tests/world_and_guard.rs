//! Opening resident occupancy, generation, and player deterrence contracts.

use super::*;

#[test]
fn opening_resident_save_rejects_materialized_wall_inside_its_body() {
    use crate::content::{FORM_BOARD, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
    use crate::core::quantity::{Area, Length};
    use crate::logistics::add_traversable_edge_for_test;
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(0, 0, 0);
    let junction = VoxelCoord::new(1, 0, 0);
    let tip = VoxelCoord::new(2, 0, 0);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    let blocking = VoxelCoord::new(0, 1, 0);
    let member = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(blocking, VoxelCoord::new(1, 2, 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(1_000_000),
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, member, FORM_BOARD);
    assert_eq!(validate_loaded_kobolds(&registries, &state), Ok(()));
    // Inject an impossible resident after legitimate materialization. The
    // normal construction owner prevents this overlap; load admission must
    // independently reject a corrupted actor position.
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    assert_eq!(
        validate_loaded_kobolds(&registries, &state),
        Err("constructed infrastructure intersects an opening kobold resident")
    );
    let corrupted = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&corrupted)
            .unwrap()
            .into_state(&registries)
            .is_err(),
        "trusted load must reject a physically impossible pre-generation resident position"
    );
}

#[test]
fn fatal_survival_tick_does_not_make_a_dead_player_guard_the_scout() {
    use crate::core::quantity::Energy;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::survival::{Vitality, player_record_with_thermal_comfort};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    let observer = VoxelCoord::new(-1, 0, 1);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    while state.tick().value() < 179 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let survival = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            Energy::ZERO,
            survival.hydration(),
            Vitality::from_parts_per_million_unchecked(
                registries
                    .survival()
                    .physiology()
                    .starvation_vitality_loss_ppm_per_tick(),
            ),
            survival.nutrition(),
            survival.vitality_recovery_remainder(),
            survival.thermal_comfort_ppm(),
        ),
    );
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &state,
        den
    ));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    let mut surviving = state.clone();
    let before = *surviving.survival().player().unwrap();
    let revision = surviving.survival().revision();
    surviving.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            registries
                .survival()
                .physiology()
                .maximum_metabolic_energy(),
            before.hydration(),
            Vitality::MAXIMUM,
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    let _ = advance_tick(&registries, &mut surviving).unwrap();
    assert_eq!(
        colony(&surviving).scout_position(),
        den,
        "a living nearby player actually keeps the scout from leaving the den"
    );

    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(state.tick(), SimulationTick::new(180));
    assert_eq!(
        state.survival().player().unwrap().vitality(),
        Vitality::ZERO
    );
    assert_eq!(
        colony(&state).scout_position(),
        junction,
        "an actor dying this tick cannot preemptively frighten a scout after death"
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, loaded);
}

#[test]
fn distant_terrain_generation_does_not_stop_an_ungenerated_opening_patrol() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::state::validate_loaded_state;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, validate_generate_chunk};

    let registries = build_registries();
    let mut opening = AppState::new();
    let den = VoxelCoord::new(0, 0, 0);
    let junction = VoxelCoord::new(1, 0, 0);
    let tip = VoxelCoord::new(2, 0, 0);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut opening, from, to);
    }
    opening
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);

    let mut remotely_generated = opening.clone();
    validate_generate_chunk(
        &registries,
        &remotely_generated,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(20, 20),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut remotely_generated)
    .unwrap();
    assert!(remotely_generated.world().generation_profile().is_some());
    assert!(
        remotely_generated
            .world()
            .get_chunk(ChunkCoord::containing(junction))
            .is_none()
    );
    assert!(crate::kobolds::patrol::scout_can_enter(
        &registries,
        &remotely_generated,
        junction,
        None,
    ));
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &remotely_generated,
        den,
        junction,
    ));

    for _ in 0..180 {
        let _ = advance_tick(&registries, &mut opening).unwrap();
        let _ = advance_tick(&registries, &mut remotely_generated).unwrap();
    }
    assert_eq!(colony(&opening).scout_position(), junction);
    assert_eq!(colony(&remotely_generated), colony(&opening));
    assert_eq!(
        validate_loaded_state(&registries, &remotely_generated),
        Ok(())
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &remotely_generated)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, remotely_generated);
}

#[test]
fn distant_chunk_cannot_impose_vertical_limits_on_ungenerated_patrol_cells() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(0, WORLD_MAX_Y_EXCLUSIVE + 1, 0);
    let junction = VoxelCoord::new(1, WORLD_MAX_Y_EXCLUSIVE + 1, 0);
    let tip = VoxelCoord::new(2, WORLD_MAX_Y_EXCLUSIVE + 1, 0);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    assert_eq!(validate_loaded_kobolds(&registries, &state), Ok(()));

    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(20, 20),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_kobolds(&registries, &state), Ok(()));
    assert!(crate::kobolds::patrol::scout_can_enter(
        &registries,
        &state,
        junction,
        None,
    ));
}

#[test]
fn terrain_generation_must_not_bury_an_existing_opening_resident() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, ChunkGenerationError, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(8, -10, 8);
    let junction = VoxelCoord::new(9, -10, 8);
    let tip = VoxelCoord::new(10, -10, 8);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);

    let before = state.clone();
    assert!(matches!(
        validate_generate_chunk(
            &registries,
            &state,
            WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
            ChunkCoord::containing(den),
            BLOCK_STATE_STONE,
        ),
        Err(ChunkGenerationError::OpeningResidentBodyCollision { voxel }) if voxel == den
    ));
    assert_eq!(state, before, "rejected generation must not modify custody");

    // Loading cannot legitimize an overlapped resident created by any other
    // state-construction path, even if the chunk itself is otherwise valid.
    let mut invalid = AppState::new();
    validate_generate_chunk(
        &registries,
        &invalid,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        ChunkCoord::containing(den),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut invalid)
    .unwrap();
    add_traversable_edge_for_test(&mut invalid, den, junction);
    add_traversable_edge_for_test(&mut invalid, junction, tip);
    invalid
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    assert!(matches!(
        validate_loaded_state(&registries, &invalid),
        Err(StateValidationError::Kobolds(
            "generated terrain intersects an opening kobold resident"
        ))
    ));

    // The same authored resident route above real ground does not reserve
    // an entire chunk or prevent valid generation and save admission.
    let mut clear = AppState::new();
    let air_den = VoxelCoord::new(8, 30, 8);
    let air_junction = VoxelCoord::new(9, 30, 8);
    let air_tip = VoxelCoord::new(10, 30, 8);
    add_traversable_edge_for_test(&mut clear, air_den, air_junction);
    add_traversable_edge_for_test(&mut clear, air_junction, air_tip);
    clear
        .kobold_state_mut()
        .apply_opening_colony([air_den, air_junction, air_tip]);
    validate_generate_chunk(
        &registries,
        &clear,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        ChunkCoord::containing(air_den),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut clear)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &clear), Ok(()));
}

#[test]
fn scout_move_invalidates_a_previously_clear_chunk_generation_plan() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, ChunkGenerationCommitError, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(15, -10, 8);
    let junction = VoxelCoord::new(16, -10, 8);
    let tip = VoxelCoord::new(17, -10, 8);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);

    // The scout is initially across the chunk border, so a solid candidate
    // chunk at the junction is legal at validation time.
    let planned = validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        ChunkCoord::containing(junction),
        BLOCK_STATE_STONE,
    )
    .unwrap();
    while state.tick().value() < 180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(colony(&state).scout_position(), junction);
    let before = state.clone();
    assert!(matches!(
        planned.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleOpeningResidents {
            expected: Some([expected_scout, _]),
            actual: Some([actual_scout, _]),
        }) if expected_scout == den && actual_scout == junction
    ));
    assert_eq!(state, before, "stale generation must not create terrain");
}

#[test]
fn distant_scout_patrol_does_not_invalidate_unrelated_chunk_generation() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(8, -10, 8);
    let junction = VoxelCoord::new(9, -10, 8);
    let tip = VoxelCoord::new(10, -10, 8);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let far_chunk = ChunkCoord::new(4, 4);
    let planned = validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        far_chunk,
        BLOCK_STATE_STONE,
    )
    .unwrap();
    while state.tick().value() < 180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(planned.commit(&mut state), Ok(far_chunk));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn distant_scout_patrol_does_not_block_player_neighborhood_streaming() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk, validate_generate_player_neighborhood,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x4B4F_424F_4C44_5445);
    for chunk in [ChunkCoord::new(0, 0), ChunkCoord::new(4, 4)] {
        validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let surface = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface + 1, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();

    // A physically supported patrol in an already generated distant column
    // continues while the player demands only the missing nearby chunks.
    let den = VoxelCoord::new(72, 80, 72);
    let junction = VoxelCoord::new(73, 80, 72);
    let tip = VoxelCoord::new(74, 80, 72);
    for location in [den, junction, tip] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(location.x(), location.y() - 1, location.z()),
            BLOCK_STATE_STONE,
        );
        for y in location.y()..=location.y() + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(location.x(), y, location.z()),
                BlockStateId::AIR,
            );
        }
    }
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let pending = validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap();
    assert_eq!(pending.chunks().len(), 8);
    while state.tick().value() < 180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(pending.commit(&mut state).unwrap().len(), 8);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn player_guards_diagonal_clearing_but_cannot_deter_through_an_opaque_corner() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, set_generated_block_for_test, validate_generate_chunk,
    };

    let (registries, mut state, _) = generated_opening();
    let scout = colony(&state).route[1];
    let observer = VoxelCoord::new(scout.x() + 1, scout.y(), scout.z() + 1);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44),
        ChunkCoord::containing(scout),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let x_corner = VoxelCoord::new(observer.x(), scout.y(), scout.z());
    let z_corner = VoxelCoord::new(scout.x(), scout.y(), observer.z());
    for position in [scout, observer, x_corner, z_corner] {
        for y in position.y()..=position.y() + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &state,
        scout
    ));

    set_generated_block_for_test(registries.world(), &mut state, x_corner, BLOCK_STATE_STONE);
    assert!(
        !crate::kobolds::patrol::living_player_deters_scout(&registries, &state, scout),
        "a player hidden behind a real opaque corner must not intimidate a scout"
    );
    set_generated_block_for_test(registries.world(), &mut state, x_corner, BlockStateId::AIR);
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &state,
        scout
    ));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
