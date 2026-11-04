//! Indexed support lookup keeps direct construction local and excludes inert plans.

use super::{
    DirectPlacementTargetError, local_active_face_supports, single_voxel_bounds,
    validate_direct_placement_position,
};
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length};
use crate::core::state::{AppState, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementGeometry, StructuralElementId, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
};
use crate::world::{BlockStateId, ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

#[test]
fn pointed_placement_rejects_both_physical_tree_trunk_cells() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        crate::content::BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let root = state.world().trees().next().unwrap().root();
    for voxel in [root, VoxelCoord::new(root.x(), root.y() + 1, root.z())] {
        assert_eq!(state.world().block_at(voxel), Some(BlockStateId::AIR));
        assert_eq!(
            validate_direct_placement_position(&registries, &state, voxel),
            Err(DirectPlacementTargetError::LocationOccupied { position: voxel }),
        );
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn depleted_living_forage_blocks_full_footprint_placement_preview() {
    use crate::core::time::SimulationTick;
    use crate::player::validate_admit_player;
    use crate::surface::SurfaceResourceLifecycle;
    use crate::world::{WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, validate_generate_player_neighborhood};

    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x464F_5241_4745_434F);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        crate::content::BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let spawn = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .map(|y| VoxelCoord::new(0, y, 0))
        .find(|&voxel| {
            state
                .world()
                .has_standing_clearance(registries.world(), voxel)
        })
        .expect("seeded origin has a supported air cell");
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("neighborhood must have ungenerated chunks")
        .commit(&mut state)
        .unwrap();
    let source = state
        .surface()
        .perennial_resources()
        .next()
        .expect("seeded wilderness has a rooted edible patch");
    let position = source.position();
    let id = source.id();
    assert_eq!(
        super::validate_structural_placement_position(&state, position),
        Err(DirectPlacementTargetError::LocationOccupied { position })
    );

    let mut plant = source.perennial.unwrap();
    plant.root_reserve = plant
        .root_reserve
        .checked_add(source.remaining_mass())
        .unwrap();
    let revision = state.surface().revision() + 1;
    state.surface_state_mut().apply_perennial_cycle(
        vec![(
            id,
            plant,
            crate::core::quantity::Mass::ZERO,
            SurfaceResourceLifecycle::Depleted,
            Some(SimulationTick::ZERO),
        )],
        revision,
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(position))
            .next()
            .is_none()
    );
    assert!(state.surface().rooted_biomass_occupies_voxel(position));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let snapshot = state.clone();
    assert_eq!(
        super::validate_structural_placement_position(&state, position),
        Err(DirectPlacementTargetError::LocationOccupied { position }),
        "dead fruit does not make an occupied building site clear"
    );
    assert_eq!(state, snapshot);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        super::validate_structural_placement_position(&restored, position),
        Err(DirectPlacementTargetError::LocationOccupied { position })
    );
}

#[test]
fn supported_timber_uses_only_live_face_neighbors_after_trusted_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let target = VoxelCoord::new(3, 2, 0);
    let positions = [
        (VoxelCoord::new(2, 2, 0), true),
        (VoxelCoord::new(3, 2, 1), true),
        (VoxelCoord::new(3, 2, -1), false),
        (VoxelCoord::new(30, 2, 0), true),
    ];
    let mut elements = Vec::<StructuralElementId>::new();
    for (position, active) in positions {
        let bounds = single_voxel_bounds(position).unwrap();
        let geometry = StructuralElementGeometry::new(
            bounds,
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(5_000),
        )
        .unwrap();
        let element = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
            true,
        )
        .unwrap();
        if active {
            materialize_structural_element_for_test(&registries, &mut state, element, FORM_LOG);
            let _ = validate_activate_structural_element(&registries, &state, element)
                .unwrap()
                .commit(&mut state)
                .unwrap();
        }
        elements.push(element);
    }
    let target_bounds = VoxelBounds::new(target, VoxelCoord::new(4, 3, 1)).unwrap();
    assert_eq!(
        local_active_face_supports(&state, target, target_bounds),
        vec![elements[0], elements[1]],
        "only two distinct active face contacts can support the placement"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(
        local_active_face_supports(&loaded, target, target_bounds),
        vec![elements[0], elements[1]],
        "the rebuilt spatial index must preserve support selection"
    );
}
