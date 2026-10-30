//! Multi-roller line shaft is physical construction, finite power and failure.

use super::*;

pub(super) fn build_roller_at(
    registries: &Registries,
    state: &mut AppState,
    cell: VoxelCoord,
) -> crate::structural::StructuralElementId {
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(registries, state).unwrap();
    }
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let stack = project_material_stacks(registries, state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))
        .unwrap();
    let bounds = VoxelBounds::new(
        cell,
        VoxelCoord::new(cell.x() + 1, cell.y() + 1, cell.z() + 1),
    )
    .unwrap();
    let geometry = StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(300),
    )
    .unwrap();
    let start = validate_start_player_grounded_structural_construction_from_stack(
        registries,
        state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
            MATERIAL_WOOD,
            geometry,
        ),
        &stack,
    )
    .expect("each straight shaft cell requires a real carried timber part");
    let member = start.element();
    let construction = start.commit(state).unwrap();
    while state.tick() < construction.completes_at() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(registries, state).unwrap();
        }
    }
    assert_eq!(
        state.structures().get_element(member).unwrap().lifecycle(),
        StructuralLifecycle::Active
    );
    member
}

#[test]
fn three_segment_timber_shaft_transmits_with_cumulative_friction_and_disconnects_when_cut() {
    let (registries, mut state, source, destination, first, _) =
        test_worksite_with_distance(false, 4);
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        handle,
        Mass::from_milligrams(600_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let baseline = calculate_matter_accounting(&state).unwrap().total();
    let mut rollers = Vec::new();
    for x in 1..=3 {
        rollers.push(build_roller_at(
            &registries,
            &mut state,
            VoxelCoord::new(x, first.y(), first.z()),
        ));
        if x < 3 {
            for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
                let tick = advance_control_tick(&registries, &mut state).unwrap();
                if let Some(slow) = tick.simulation() {
                    assert!(
                        slow.mechanical_transmissions().is_empty(),
                        "incomplete shaft must not teleport work over absent rollers",
                    );
                }
            }
        }
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(handle),
        Mass::from_milligrams(15_000),
        "each built timber roller embodies 195 g of carried wood",
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        baseline
    );

    let source_before = state.energy().get_store(source).unwrap().stored();
    let dest_before = state.energy().get_store(destination).unwrap().stored();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut transmission = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let result = advance_control_tick(&registries, &mut state).unwrap();
        let replay = advance_control_tick(&registries, &mut loaded).unwrap();
        assert_eq!(result.simulation().is_some(), replay.simulation().is_some());
        if let Some(slow) = result.simulation() {
            assert_eq!(slow.mechanical_transmissions().len(), 1);
            transmission = slow.mechanical_transmissions().first().copied();
        }
    }
    assert_eq!(
        state, loaded,
        "multi-roller settlement must survive save/replay"
    );
    let transfer = transmission.expect("completed three-part shaft must transmit");
    assert_eq!(transfer.coupling(), rollers[0]);
    assert_eq!(transfer.segments(), 3);
    assert_eq!(transfer.source(), source);
    assert_eq!(transfer.destination(), destination);
    assert_eq!(
        transfer.delivered().nanojoules(),
        transfer.drawn().nanojoules() * 512 / 1_000,
        "three 80% stages must lose 48.8% of shaft input",
    );
    assert_eq!(
        source_before.checked_sub(state.energy().get_store(source).unwrap().stored()),
        Some(transfer.drawn())
    );
    assert_eq!(
        state
            .energy()
            .get_store(destination)
            .unwrap()
            .stored()
            .checked_sub(dest_before),
        Some(transfer.delivered())
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        baseline
    );

    // Breaking the center roller interrupts the whole run. Source and sink
    // have not moved and there is no invisible connection to bridge the gap.
    let break_work = crate::structural::validate_start_player_structural_dismantling(
        &registries,
        &state,
        rollers[1],
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    while state.tick() < break_work.completes_at() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let tick = advance_control_tick(&registries, &mut state).unwrap();
            if let Some(slow) = tick.simulation()
                && slow.tick() == break_work.completes_at()
            {
                assert!(
                    slow.mechanical_transmissions().is_empty(),
                    "completed center removal must break the shaft before this tick settles",
                );
            }
        }
    }
    assert!(state.structures().get_element(rollers[1]).is_none());
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let tick = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(slow) = tick.simulation() {
            assert!(slow.mechanical_transmissions().is_empty());
        }
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        baseline
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
