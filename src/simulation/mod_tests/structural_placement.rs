//! Pointed construction distinguishes slender post overlap from full-cell obstruction.

use super::*;
use crate::content::{FORM_BOARD, FORM_CHIP, FORM_HANDLE, MATERIAL_STONE};
use crate::logistics::{PlayerActiveSelection, validate_set_active_selection};
use crate::material::MaterialComposition;
use crate::matter::calculate_matter_accounting;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, insert_surface_resource_for_test};

#[test]
fn impossible_timber_use_rejects_on_input_tick_instead_of_queuing() {
    for form in [FORM_LOG, FORM_BOARD, FORM_HANDLE] {
        let (registries, mut state, _) = generated_grounded_player_at(0, 0);
        let commodity = CommodityKey::new(MATERIAL_WOOD, form);
        let carried = state.logistics().player().unwrap().carried_stockpile();
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            Mass::from_milligrams(6_500_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        validate_set_active_selection(
            &registries,
            &state,
            Some(PlayerActiveSelection::material(commodity)),
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let upward = ViewDirection::new(0, 1_000_000, 0).unwrap();
        assert_eq!(
            crate::world::raycast_player_block(&registries, &state, upward).unwrap(),
            None,
            "the aim must not hit a real world surface"
        );
        let outcome = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::use_active(upward)),
        )
        .unwrap();
        assert_eq!(
            outcome.secondary_action(),
            Some(&SecondaryActionOutcome::NoStructuralPlacementTarget),
            "impossible timber placement must not reserve a future action for {form:?}"
        );
        assert!(state.logistics().pending_secondary_use().is_none());
        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );
    }
}

#[test]
fn side_wall_fire_refueling_uses_multiple_valid_timber_lots_without_placement_ambiguity() {
    use crate::content::{CAMPFIRE_FUEL_THERMAL_ENERGY, ENERGY_STONE_RING_CAMPFIRE, FORM_LUMP};
    use crate::energy::validate_assemble_energy_store;
    use crate::inventory::project_material_stacks;

    let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
    let wall = VoxelCoord::new(2, foot_y + 1, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(1, foot_y + 1, 0),
        BlockStateId::AIR,
    );
    set_generated_block_for_test(registries.world(), &mut state, wall, BLOCK_STATE_STONE);
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for temperature in [293_150, 303_150] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            timber,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(temperature),
        )
        .unwrap();
    }
    assert_eq!(
        project_material_stacks(&registries, &state, carried)
            .unwrap()
            .into_iter()
            .filter(|stack| stack.commodity() == timber)
            .count(),
        2
    );
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let mut result = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        result.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        result = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        result.secondary_action(),
        Some(&SecondaryActionOutcome::CombustionFuelAdded {
            store: fire,
            released: CAMPFIRE_FUEL_THERMAL_ENERGY,
        })
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn pointed_supported_post_retains_priority_over_local_refueling() {
    use crate::content::{CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_LUMP};
    use crate::energy::validate_assemble_energy_store;

    let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
    let ground = VoxelCoord::new(1, foot_y - 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, ground, BLOCK_STATE_STONE);
    for y in foot_y..=foot_y + 1 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(1, y, 0),
            BlockStateId::AIR,
        );
    }
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        timber,
        Mass::from_milligrams(6_500_000)
            .checked_add(CAMPFIRE_FUEL_MASS)
            .unwrap(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let direction = ViewDirection::new(600_000, -800_000, 0).unwrap();
    assert_eq!(
        crate::world::raycast_player_block(&registries, &state, direction)
            .unwrap()
            .map(|target| (target.voxel(), target.face())),
        Some((ground, crate::spatial::CubeFace::Top))
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(direction));
    let first = advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        first.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let mut completed = first;
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        completed = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert!(matches!(
        completed.secondary_action(),
        Some(SecondaryActionOutcome::StructuralConstructionStarted(_))
    ));
    assert_eq!(
        state.energy().get_store(fire).unwrap().stored(),
        Energy::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn direct_timber_post_can_share_a_cell_with_loose_matter() {
    for occupied in [false, true] {
        let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
        let ground = VoxelCoord::new(1, foot_y - 1, 0);
        let destination = VoxelCoord::new(1, foot_y, 0);
        set_generated_block_for_test(registries.world(), &mut state, ground, BLOCK_STATE_STONE);
        for y in foot_y..=foot_y + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(1, y, 0),
                BlockStateId::AIR,
            );
        }
        let loose = occupied.then(|| {
            let spec = SurfaceResourceSpec::new(
                SurfaceResourceKind::Drop,
                destination,
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(50_000),
                Temperature::from_millikelvin(293_150),
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap();
            insert_surface_resource_for_test(&registries, &mut state, spec).unwrap()
        });
        let carried = state.logistics().player().unwrap().carried_stockpile();
        let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            timber,
            Mass::from_milligrams(6_500_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        validate_set_active_selection(
            &registries,
            &state,
            Some(PlayerActiveSelection::material(timber)),
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let direction = ViewDirection::new(600_000, -800_000, 0).unwrap();
        assert_eq!(
            crate::world::raycast_player_block(&registries, &state, direction)
                .unwrap()
                .map(|target| (target.voxel(), target.face())),
            Some((ground, crate::spatial::CubeFace::Top))
        );
        let matter_before = calculate_matter_accounting(&state).unwrap().total();
        let surface_before = state.surface().clone();
        let mut outcome = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::use_active(direction)),
        )
        .unwrap();
        assert_eq!(
            outcome.secondary_action(),
            Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
        );
        for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
            outcome = advance_control_tick(&registries, &mut state).unwrap();
        }
        assert!(
            matches!(
                outcome.secondary_action(),
                Some(SecondaryActionOutcome::StructuralConstructionStarted(_))
            ),
            "slender post did not start construction: {:?}",
            outcome.secondary_action()
        );
        assert_eq!(state.surface(), &surface_before);
        if let Some(resource) = loose {
            assert_eq!(
                state.surface().get(resource).unwrap().remaining_mass(),
                Mass::from_milligrams(50_000)
            );
        }
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            matter_before
        );
        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );
    }
}

#[test]
fn inert_blueprint_does_not_block_pointed_grounded_post_construction() {
    use crate::core::quantity::{Area, Length};
    use crate::core::state::validate_loaded_state;
    use crate::spatial::VoxelBounds;
    use crate::structural::{StructuralElementGeometry, add_structural_element};

    let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
    let planned_position = VoxelCoord::new(1, foot_y, 0);
    let terrain_target = VoxelCoord::new(1, foot_y - 1, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        planned_position,
        BlockStateId::AIR,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        terrain_target,
        BLOCK_STATE_STONE,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(1, foot_y + 1, 0),
        BlockStateId::AIR,
    );
    let blueprint = add_structural_element(
        &registries,
        &mut state,
        crate::content::STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(planned_position, VoxelCoord::new(2, foot_y + 1, 1)).unwrap(),
            Length::from_micrometers(2_000_000),
            Area::from_square_millimeters(5_000),
        )
        .unwrap(),
        true,
    )
    .unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(6_500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let direction = ViewDirection::new(600_000, -800_000, 0).unwrap();
    assert_eq!(
        crate::world::raycast_player_block(&registries, &state, direction)
            .unwrap()
            .map(|target| (target.voxel(), target.face())),
        Some((terrain_target, crate::spatial::CubeFace::Top))
    );
    let before_mass = calculate_matter_accounting(&state).unwrap().total();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(direction));
    let mut response =
        advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        response.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        response = advance_control_tick(&registries, &mut state).unwrap();
    }
    let work = match response.secondary_action() {
        Some(SecondaryActionOutcome::StructuralConstructionStarted(work)) => *work,
        other => panic!("pointed terrain placement was obstructed by an inert plan: {other:?}"),
    };
    assert_ne!(work.element(), blueprint);
    assert!(
        state
            .structures()
            .get_element(blueprint)
            .unwrap()
            .embodied_material()
            .is_empty()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_mass
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn impossible_side_wall_post_does_not_shadow_an_existing_local_fire() {
    use crate::content::{
        CAMPFIRE_FUEL_MASS, CAMPFIRE_FUEL_THERMAL_ENERGY, ENERGY_STONE_RING_CAMPFIRE, FORM_LUMP,
    };
    use crate::energy::validate_assemble_energy_store;

    let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
    let wall = VoxelCoord::new(2, foot_y + 1, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(1, foot_y + 1, 0),
        BlockStateId::AIR,
    );
    set_generated_block_for_test(registries.world(), &mut state, wall, BLOCK_STATE_STONE);
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(
        state.energy().get_store(fire).unwrap().stored(),
        Energy::ZERO
    );
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        timber,
        CAMPFIRE_FUEL_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    assert_eq!(
        crate::world::raycast_player_block(&registries, &state, direction)
            .unwrap()
            .map(|target| (target.voxel(), target.face())),
        Some((wall, crate::spatial::CubeFace::West))
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(direction));
    let mut outcome =
        advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert_eq!(
        outcome,
        advance_control_tick_with_direct_input(&registries, &mut replay, input).unwrap()
    );
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        outcome = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            outcome,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::CombustionFuelAdded {
            store: fire,
            released: CAMPFIRE_FUEL_THERMAL_ENERGY,
        })
    );
    assert_eq!(
        state.energy().get_store(fire).unwrap().stored(),
        CAMPFIRE_FUEL_THERMAL_ENERGY
    );
    assert_eq!(state, replay);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
