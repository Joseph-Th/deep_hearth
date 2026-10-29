//! Physical walls block finite light while real detours continue to transmit it.

use std::collections::{BTreeSet, VecDeque};

use crate::content::{
    BLOCK_STATE_STONE, CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_LOG, FORM_LUMP,
    MATERIAL_STONE, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length, Mass};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::{validate_assemble_energy_store, validate_combust_fuel};
use crate::inventory::deposit_bulk_for_test;
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::{CubeFace, VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::world::{
    BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
    validate_generate_chunk,
};

use super::{assess_local_illumination, voxel_transmits_light};

/// Independent, unpruned shortest-path oracle for verifying target-directed
/// illumination against opaque terrain, constructed walls and real detours.
fn unpruned_light_level(
    registries: &crate::registry::Registries,
    state: &AppState,
    source: VoxelCoord,
    brightness: u8,
    target: VoxelCoord,
) -> u8 {
    let mut visited = BTreeSet::from([source]);
    let mut frontier = VecDeque::from([(source, brightness)]);
    while let Some((position, level)) = frontier.pop_front() {
        if position == target {
            return level;
        }
        if level <= 1 {
            continue;
        }
        for face in [
            CubeFace::Top,
            CubeFace::Bottom,
            CubeFace::North,
            CubeFace::South,
            CubeFace::East,
            CubeFace::West,
        ] {
            let Some(adjacent) = face.adjacent_voxel(position) else {
                continue;
            };
            if voxel_transmits_light(registries, state, adjacent) && visited.insert(adjacent) {
                frontier.push_back((adjacent, level - 1));
            }
        }
    }
    0
}

fn assert_light_matches_unpruned_paths(
    registries: &crate::registry::Registries,
    state: &AppState,
    source: VoxelCoord,
    brightness: u8,
) {
    for y in -11..=-9 {
        for z in 5..=11 {
            for x in 5..=13 {
                let target = VoxelCoord::new(x, y, z);
                assert_eq!(
                    assess_local_illumination(registries, state, target).level(),
                    unpruned_light_level(registries, state, source, brightness, target),
                    "light at {target:?} must equal the unobstructed shortest-path oracle"
                );
            }
        }
    }
}

#[test]
fn built_wall_shades_the_far_side_until_an_actual_air_bypass_opens() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4C49_4748_5457_414C),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let source = VoxelCoord::new(8, -10, 8);
    let barrier = VoxelCoord::new(9, -10, 8);
    let shaded = VoxelCoord::new(10, -10, 8);
    for at in [
        source,
        barrier,
        shaded,
        VoxelCoord::new(source.x(), source.y() + 1, source.z()),
    ] {
        set_generated_block_for_test(registries.world(), &mut state, at, BlockStateId::AIR);
    }
    let carried = validate_admit_player(&registries, &state, source)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
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
        deposit_bulk_for_test(&registries, &mut state, carried, commodity, mass).unwrap();
    }
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let brightness = assess_local_illumination(&registries, &state, source).level();
    assert!(brightness >= 5);
    assert_eq!(
        assess_local_illumination(&registries, &state, shaded).level(),
        brightness - 2
    );

    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(barrier, VoxelCoord::new(10, -9, 9)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(1_000_000),
    )
    .unwrap();
    let member = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    // An unbuilt plan has no thermal or optical matter.
    assert_eq!(
        assess_local_illumination(&registries, &state, shaded).level(),
        brightness - 2
    );
    materialize_structural_element_for_test(&registries, &mut state, member, FORM_LOG);
    assert_eq!(
        assess_local_illumination(&registries, &state, shaded).level(),
        0,
        "light cannot shine through a built wall in solid rock"
    );
    assert_light_matches_unpruned_paths(&registries, &state, source, brightness);
    let _ = validate_activate_structural_element(&registries, &state, member)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    for at in [
        VoxelCoord::new(8, -10, 9),
        VoxelCoord::new(9, -10, 9),
        VoxelCoord::new(10, -10, 9),
    ] {
        set_generated_block_for_test(registries.world(), &mut state, at, BlockStateId::AIR);
    }
    assert_eq!(
        assess_local_illumination(&registries, &state, shaded).level(),
        brightness - 4,
        "light can route around the wall only along carved air cells"
    );
    assert_light_matches_unpruned_paths(&registries, &state, source, brightness);
    // Open a wider gallery without removing the wall. This exposes many
    // competing paths that the production search now prunes; the independent
    // flood fill must still agree across the whole lit and unlit neighborhood.
    for z in 5..=11 {
        for x in 5..=13 {
            let voxel = VoxelCoord::new(x, -10, z);
            if voxel != barrier {
                set_generated_block_for_test(
                    registries.world(),
                    &mut state,
                    voxel,
                    BlockStateId::AIR,
                );
            }
        }
    }
    assert_eq!(
        assess_local_illumination(&registries, &state, shaded).level(),
        brightness - 4,
        "opening a gallery must not allow light to pass through the installed wall"
    );
    assert_light_matches_unpruned_paths(&registries, &state, source, brightness);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(validate_loaded_state(&registries, &restored), Ok(()));
    assert_eq!(
        assess_local_illumination(&registries, &restored, shaded).level(),
        brightness - 4
    );
}
