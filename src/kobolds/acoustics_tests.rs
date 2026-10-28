//! Bounded stone-and-air hearing contracts and save continuation for local clatter.

use super::*;

use crate::content::{
    BLOCK_STATE_STONE, ENERGY_ELECTRICAL_BUFFER, FORM_ELECTRICAL_WINDING, FORM_HANDLE, FORM_LOG,
    MATERIAL_COPPER, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::logistics::{add_traversable_edge_for_test, place_energy_store_for_fixture};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};
use crate::world::{
    BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
    validate_generate_chunk,
};

fn solid_generated_chunk() -> (Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4143_4F55_5354_4943),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    (registries, state)
}

#[test]
fn generated_tree_trunk_occludes_local_sight_and_short_acoustic_paths() {
    let (registries, state) = solid_generated_chunk();
    let tree = state
        .world()
        .trees()
        .next()
        .expect("seeded stone chunk has a tree");
    let root = tree.root();
    let left = VoxelCoord::new(root.x() - 1, root.y() + 1, root.z());
    let right = VoxelCoord::new(root.x() + 1, root.y() + 1, root.z());
    let trunk = VoxelCoord::new(root.x(), root.y() + 1, root.z());
    assert_eq!(state.world().block_at(left), Some(BlockStateId::AIR));
    assert_eq!(state.world().block_at(right), Some(BlockStateId::AIR));
    assert_eq!(state.world().block_at(trunk), Some(BlockStateId::AIR));
    assert!(state.world().tree_trunk_at(trunk).is_some());

    assert!(
        !crate::kobolds::visibility::scout_visible_from(&registries, &state, left, right),
        "wood cannot become transparent merely because the terrain block is air"
    );
    assert!(
        !audible_path(
            &registries,
            &state,
            left,
            right,
            KoboldJunkRigKind::TimberRatchet,
        ),
        "a short, face-connected sound path cannot pass through a solid trunk"
    );
}

fn carve(registries: &Registries, state: &mut AppState, voxels: &[VoxelCoord]) {
    for &voxel in voxels {
        assert_eq!(state.world().block_at(voxel), Some(BLOCK_STATE_STONE));
        set_generated_block_for_test(registries.world(), state, voxel, BlockStateId::AIR);
    }
}

fn drop_part(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    commodity: CommodityKey,
) {
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        commodity,
        Mass::from_milligrams(50_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(commodity.material()),
    )
    .unwrap();
    validate_create_surface_resources(registries, state, [spec])
        .unwrap()
        .commit(state)
        .unwrap();
}

#[test]
fn distant_extreme_coordinates_do_not_wrap_local_sight_or_hearing() {
    let registries = build_registries();
    let state = AppState::new();
    let east = VoxelCoord::new(i64::MAX, 0, 0);
    let near = VoxelCoord::new(i64::MAX - 2, 0, 0);
    let within_iron = VoxelCoord::new(i64::MAX - 6, 0, 0);
    let beyond_iron = VoxelCoord::new(i64::MAX - 7, 0, 0);
    let opposite_extreme = VoxelCoord::new(i64::MIN, 0, 0);
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        east,
        near,
    ));
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        east,
        within_iron,
    ));
    assert!(audible_path(
        &registries,
        &state,
        east,
        within_iron,
        KoboldJunkRigKind::IronChime,
    ));
    assert!(!audible_path(
        &registries,
        &state,
        east,
        within_iron,
        KoboldJunkRigKind::StoneClatter,
    ));
    assert!(!audible_path(
        &registries,
        &state,
        east,
        beyond_iron,
        KoboldJunkRigKind::IronChime,
    ));
    assert!(!audible_path(
        &registries,
        &state,
        east,
        opposite_extreme,
        KoboldJunkRigKind::IronChime,
    ));
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        east,
        opposite_extreme,
    ));
    // Authored graph mode has no generated voxel opacity, but cannot leak
    // perception beyond the same bounded distance admitted in generated terrain.
    let south = VoxelCoord::new(i64::MIN, i64::MIN, i64::MIN);
    let near_south = VoxelCoord::new(i64::MIN + 1, i64::MIN + 1, i64::MIN);
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        south,
        near_south,
    ));
}

#[test]
fn ungenerated_full_cell_wall_forces_actual_bounded_sound_detour() {
    use crate::content::FORM_BOARD;

    let registries = build_registries();
    let mut state = AppState::new();
    let source = VoxelCoord::new(0, 0, 0);
    let listener = VoxelCoord::new(2, 0, 0);
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet,
    ));
    let member = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(1_000_000),
        )
        .unwrap(),
        true,
    )
    .unwrap();
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet,
    ));
    materialize_structural_element_for_test(&registries, &mut state, member, FORM_BOARD);
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet,
    ));
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter,
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert!(!audible_path(
        &registries,
        &loaded,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet,
    ));
}

#[test]
fn a_generated_stone_corner_hides_the_origin_of_a_real_two_hop_warning() {
    let (registries, mut state) = solid_generated_chunk();
    let den = VoxelCoord::new(8, -10, 8);
    let junction = VoxelCoord::new(9, -10, 8);
    let tip = VoxelCoord::new(10, -10, 8);
    let listener = VoxelCoord::new(9, -10, 9);
    for position in [den, junction, tip, listener] {
        carve(
            &registries,
            &mut state,
            &[
                position,
                VoxelCoord::new(position.x(), position.y() + 1, position.z()),
            ],
        );
    }
    assert_eq!(
        state.world().block_at(VoxelCoord::new(8, -10, 9)),
        Some(BLOCK_STATE_STONE)
    );
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let machinery = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, machinery, junction);
    while state.tick().value() < 205 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.kobold_state().colony.as_ref().unwrap().scout_step, 2);
    for position in [den, junction] {
        drop_part(
            &registries,
            &mut state,
            position,
            CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
        );
        drop_part(
            &registries,
            &mut state,
            position,
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        );
    }
    validate_admit_player(&registries, &state, listener)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    // A report from an earlier encounter awaits a valid shared machine beat.
    // It is legal while the scout remains at the real outer patrol post.
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .watch_alarm_pending = true;
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        den,
        listener
    ));
    let matter = calculate_matter_accounting(&state).unwrap().total();
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut heard = Vec::new();
    let mut replayed = Vec::new();
    for (branch, sounds) in [(&mut state, &mut heard), (&mut replay, &mut replayed)] {
        while branch.tick().value() < 210 {
            let control = advance_control_tick(&registries, branch).unwrap();
            if let Some(slow) = control.simulation() {
                sounds.extend_from_slice(slow.kobold_alarm_pulses());
            }
        }
    }
    assert_eq!(state, replay);
    assert_eq!(heard, replayed);
    assert_eq!(
        heard
            .iter()
            .map(|pulse| pulse.position())
            .collect::<Vec<_>>(),
        vec![None, Some(junction)],
        "the hidden den clatters around rock, but only the visible junction can be located"
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn around_a_rock_corner_a_clatter_is_audible_but_not_visible() {
    let (registries, mut state) = solid_generated_chunk();
    let source = VoxelCoord::new(8, -10, 8);
    let corner = VoxelCoord::new(8, -10, 9);
    let listener = VoxelCoord::new(9, -10, 9);
    carve(&registries, &mut state, &[source, corner, listener]);
    assert_eq!(
        state.world().block_at(VoxelCoord::new(9, -10, 8)),
        Some(BLOCK_STATE_STONE)
    );
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        source,
        listener
    ));
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet
    ));
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(replay, state);
    assert!(audible_path(
        &registries,
        &replay,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet
    ));
    set_generated_block_for_test(registries.world(), &mut state, corner, BLOCK_STATE_STONE);
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn bare_stone_carries_five_air_steps_but_a_damped_timber_ratchet_does_not() {
    let (registries, mut state) = solid_generated_chunk();
    let path = [
        VoxelCoord::new(8, -10, 8),
        VoxelCoord::new(9, -10, 8),
        VoxelCoord::new(10, -10, 8),
        VoxelCoord::new(11, -10, 8),
        VoxelCoord::new(11, -10, 9),
        VoxelCoord::new(11, -10, 10),
    ];
    carve(&registries, &mut state, &path);
    let source = path[0];
    let listener = path[5];
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter
    ));
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet
    ));
    set_generated_block_for_test(registries.world(), &mut state, path[3], BLOCK_STATE_STONE);
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter
    ));
}

#[test]
fn recovered_iron_chime_is_heard_one_cell_beyond_stone_but_not_through_rock() {
    let (registries, mut state) = solid_generated_chunk();
    let corridor = (8..=14)
        .map(|x| VoxelCoord::new(x, -10, 8))
        .collect::<Vec<_>>();
    carve(&registries, &mut state, &corridor);
    let source = corridor[0];
    let listener = corridor[6];
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::IronChime
    ));
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter
    ));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert!(audible_path(
        &registries,
        &replay,
        source,
        listener,
        KoboldJunkRigKind::IronChime
    ));
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        corridor[3],
        BLOCK_STATE_STONE,
    );
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::IronChime
    ));
}

#[test]
fn missing_chunk_is_never_assumed_to_be_an_open_sound_passage() {
    let (registries, mut state) = solid_generated_chunk();
    let source = VoxelCoord::new(15, -10, 8);
    let outside = VoxelCoord::new(16, -10, 8);
    carve(&registries, &mut state, &[source]);
    assert_eq!(state.world().block_at(outside), None);
    assert!(!audible_path(
        &registries,
        &state,
        source,
        outside,
        KoboldJunkRigKind::StoneClatter
    ));
}

#[test]
fn materialized_member_obscures_a_scout_and_blocks_air_sound_until_a_bypass_opens() {
    let (registries, mut state) = solid_generated_chunk();
    let source = VoxelCoord::new(8, -10, 8);
    let barrier = VoxelCoord::new(9, -10, 8);
    let listener = VoxelCoord::new(10, -10, 8);
    carve(&registries, &mut state, &[source, barrier, listener]);
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

    // The plan is not a wall until it owns physical material.
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        source,
        listener
    ));
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet
    ));
    materialize_structural_element_for_test(&registries, &mut state, member, FORM_LOG);
    assert!(state.structures().blocks_full_cell(barrier));
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        source,
        listener
    ));
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter
    ));
    assert!(!crate::kobolds::patrol::scout_can_enter(
        &registries,
        &state,
        barrier,
        None
    ));
    let _ = validate_activate_structural_element(&registries, &state, member)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    // Sound can still travel along a real four-edge air path around the member.
    carve(
        &registries,
        &mut state,
        &[
            VoxelCoord::new(8, -10, 9),
            VoxelCoord::new(9, -10, 9),
            VoxelCoord::new(10, -10, 9),
        ],
    );
    assert!(audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter
    ));
    assert!(!audible_path(
        &registries,
        &state,
        source,
        listener,
        KoboldJunkRigKind::TimberRatchet
    ));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(validate_loaded_state(&registries, &restored), Ok(()));
    assert!(audible_path(
        &registries,
        &restored,
        source,
        listener,
        KoboldJunkRigKind::StoneClatter
    ));
}
