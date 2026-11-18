//! Physical sight is neither remote custody nor an omniscient material map.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_LUMP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::material::MaterialComposition;
use crate::player::validate_admit_player;
use crate::surface::{
    SurfaceResourceSpec, player_surface_gathering_options, validate_create_surface_resources,
};
use crate::survival::player_record_with_thermal_comfort;
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WorldGenerationSeed, set_generated_block_for_test,
    validate_generate_chunk,
};

#[test]
fn distant_surface_sight_is_bounded_lit_unobstructed_and_does_not_grant_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(31),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let y = WORLD_MAX_Y_EXCLUSIVE - 4;
    let origin = VoxelCoord::new(1, y, 2);
    let near = VoxelCoord::new(2, y, 2);
    let far = VoxelCoord::new(7, y, 2);
    let outside = VoxelCoord::new(14, y, 2);
    for position in [origin, near, far, outside] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(position.x(), y - 1, position.z()),
            BLOCK_STATE_STONE,
        );
    }
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let specs = [near, far, outside].map(|position| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    });
    validate_create_surface_resources(&registries, &state, specs)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let observe = |state: &AppState| available_visible_surface_landmarks(&registries, state);
    assert!(observe(&state).is_empty());
    validate_admit_player(&registries, &state, origin)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let unchanged = state.clone();
    assert_eq!(
        observe(&state),
        vec![SurfaceResourceLandmark {
            position: far,
            commodity
        }]
    );
    assert!(
        player_surface_gathering_options(&registries, &state)
            .unwrap()
            .is_empty()
    );
    assert_eq!(state, unchanged);
    let mut obstructed = state.clone();
    for wall_y in [y, y + 1] {
        set_generated_block_for_test(
            registries.world(),
            &mut obstructed,
            VoxelCoord::new(5, wall_y, 2),
            BLOCK_STATE_STONE,
        );
    }
    assert!(observe(&obstructed).is_empty());
    let mut dead = state.clone();
    let living = *dead.survival().player().unwrap();
    let revision = dead.survival().revision();
    dead.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
            living.thermal_comfort_ppm(),
        ),
    );
    assert!(observe(&dead).is_empty());
    apply_clock_advance(
        &mut state,
        SimulationTick::new(registries.core().calendar().ticks_per_day() * 3 / 4),
    );
    assert!(observe(&state).is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
