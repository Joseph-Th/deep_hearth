//! Contracts for bounded visual neighborhood geometry at coordinate extremes.

use std::collections::BTreeSet;

use super::*;

#[test]
fn explicit_event_skylight_observes_dusk_and_dawn_without_changing_committed_time() {
    use crate::content::{BLOCK_STATE_STONE, build_registries};
    use crate::core::state::{AppState, apply_clock_advance};
    use crate::core::time::SimulationTick;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WorldGenerationSeed,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x534B_594C_4947_4854),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let observer = VoxelCoord::new(6, WORLD_MAX_Y_EXCLUSIVE - 2, 6);
    let target = VoxelCoord::new(7, WORLD_MAX_Y_EXCLUSIVE - 2, 6);
    assert_eq!(state.world().block_at(observer), Some(BlockStateId::AIR));
    assert_eq!(state.world().block_at(target), Some(BlockStateId::AIR));

    let day = registries.core().calendar().ticks_per_day();
    let dusk = SimulationTick::new(day * 3 / 4);
    let dawn = SimulationTick::new(day);
    apply_clock_advance(&mut state, SimulationTick::new(dusk.value() - 1));
    assert!(player_can_see_local_cell(
        &registries,
        &state,
        observer,
        target
    ));
    assert!(!player_can_see_local_cell_at(
        &registries,
        &state,
        observer,
        target,
        dusk
    ));
    assert_eq!(state.tick().value(), dusk.value() - 1);

    apply_clock_advance(&mut state, SimulationTick::new(dawn.value() - 1));
    assert!(!player_can_see_local_cell(
        &registries,
        &state,
        observer,
        target
    ));
    assert!(player_can_see_local_cell_at(
        &registries,
        &state,
        observer,
        target,
        dawn
    ));
    assert_eq!(state.tick().value(), dawn.value() - 1);
}

#[test]
fn local_observation_envelope_is_unique_bounded_and_checked_at_world_limits() {
    let center = VoxelCoord::new(0, 0, 0);
    let cells = nearby_two_step_cells(center).collect::<Vec<_>>();
    assert_eq!(cells.len(), 25);
    assert!(cells.contains(&center));
    assert_eq!(cells.iter().copied().collect::<BTreeSet<_>>().len(), 25);
    assert!(
        cells
            .iter()
            .all(|cell| within_manhattan_reach(center, *cell, 2))
    );
    for limit in [i64::MIN, i64::MAX] {
        let edge = VoxelCoord::new(limit, limit, limit);
        let near_edge = nearby_two_step_cells(edge).collect::<Vec<_>>();
        assert_eq!(near_edge.len(), 10);
        assert!(
            near_edge
                .iter()
                .all(|cell| within_manhattan_reach(edge, *cell, 2))
        );
    }
}
