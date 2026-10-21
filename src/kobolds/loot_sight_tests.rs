//! Real line of sight, not remote resource ownership, supplies detour knowledge.

use crate::content::{BLOCK_STATE_STONE, FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};
use crate::world::{
    BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
    validate_generate_chunk,
};

use super::reachable_far_drop;

#[test]
fn opaque_terrain_hides_a_far_cache_until_the_route_is_exposed() {
    let registries = build_registries();
    let mut starting = AppState::new();
    let den = VoxelCoord::new(1, 0, 0);
    let junction = VoxelCoord::new(2, 0, 0);
    let tip = VoxelCoord::new(3, 0, 0);
    let first = VoxelCoord::new(2, 0, 1);
    let goal = VoxelCoord::new(3, 0, 1);
    for (from, to) in [
        (den, junction),
        (junction, tip),
        (junction, first),
        (first, goal),
    ] {
        crate::logistics::add_traversable_edge_for_test(&mut starting, from, to);
    }
    starting
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    validate_generate_chunk(
        &registries,
        &starting,
        WorldGenerationSeed::new(7),
        ChunkCoord::containing(den),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut starting)
    .unwrap();
    for position in [den, junction, tip, first, goal] {
        set_generated_block_for_test(
            registries.world(),
            &mut starting,
            VoxelCoord::new(position.x(), -1, position.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut starting,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let plan = validate_create_surface_resources(
        &registries,
        &starting,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            goal,
            commodity,
            Mass::from_milligrams(250_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    let drop = plan.first_resource_id();
    plan.commit(&mut starting).unwrap();

    let mut visible = starting.clone();
    let mut hidden = starting;
    set_generated_block_for_test(registries.world(), &mut hidden, tip, BLOCK_STATE_STONE);
    let choose = |state: &AppState| {
        reachable_far_drop(
            &registries,
            state,
            junction,
            den,
            tip,
            None,
            Mass::from_milligrams(500_000),
            state.tick(),
            false,
        )
    };
    assert_eq!(choose(&visible), Some((first, goal)));
    assert_eq!(choose(&hidden), None);
    let visible_mass = calculate_matter_accounting(&visible).unwrap().total();
    let hidden_mass = calculate_matter_accounting(&hidden).unwrap().total();

    for _ in 0..185 {
        let _ = advance_tick(&registries, &mut visible).unwrap();
        let _ = advance_tick(&registries, &mut hidden).unwrap();
    }
    assert_eq!(
        visible
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        first,
        "the scout investigates a visible two-edge cache"
    );
    assert_eq!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        den,
        "an unseen cache cannot cause an omniscient detour around a blocked tip"
    );
    assert_eq!(hidden.surface().get(drop).unwrap().position(), goal);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &hidden)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    // An actual change in the world, not elapsed time or a global alert, allows
    // the scout to select the previously hidden alternate supply route.
    for state in [&mut hidden, &mut restored] {
        set_generated_block_for_test(registries.world(), state, tip, BlockStateId::AIR);
    }
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut hidden).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(hidden, restored);
    assert_eq!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        first
    );
    assert_eq!(
        calculate_matter_accounting(&visible).unwrap().total(),
        visible_mass
    );
    // The hidden-world baseline included a physical wall; opening it removes
    // its represented terrain mass, independently of conserved scout loot.
    assert!(calculate_matter_accounting(&hidden).unwrap().total() < hidden_mass);
    assert_eq!(hidden.surface().get(drop).unwrap().position(), goal);
    assert_eq!(validate_loaded_state(&registries, &hidden), Ok(()));
}
