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
        (0..transfer.segments()).fold(transfer.drawn().nanojoules(), |work, _| {
            crate::core::arithmetic::scale_u128_fraction_floor(work, 4, 5)
        }),
        "each physical roller loses 20% of incoming work with exact stage-wise rounding",
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

#[test]
fn shaft_transfer_cannot_spend_energy_revision_reserved_for_pending_manual_power() {
    use crate::content::MATERIAL_WOOD;
    use crate::structural::{
        add_structural_element, materialize_structural_element_for_test,
        validate_activate_structural_element,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(100_000_000_000),
    )
    .unwrap();
    let destination = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        Energy::ZERO,
    )
    .unwrap();
    let work_store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::ZERO,
    )
    .unwrap();
    let crank = assemble_crank(&registries, &mut state);
    let work_position = VoxelCoord::new(0, 0, 2);
    place_energy_store_for_fixture(&mut state, source, VoxelCoord::new(0, 0, 0));
    place_energy_store_for_fixture(&mut state, destination, VoxelCoord::new(2, 0, 0));
    place_energy_store_for_fixture(&mut state, work_store, work_position);
    crate::logistics::place_equipment_for_fixture(&mut state, crank, work_position);
    let roller = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(300),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, roller, FORM_HANDLE);
    let _ = validate_activate_structural_element(&registries, &state, roller)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, work_position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    // A real shaft transfers while its unrelated manual-power job is not
    // underway, but must not spend that job's reserved completion revision.
    let mut unreserved = state.clone();
    let work = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            work_store,
            Energy::from_nanojoules(400_000_000_000),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(work.completes_at().value() > state.tick().value() + 1);
    assert_eq!(state.checked_future_energy_revision_demand(), Some(1));
    assert!(state.energy().get_store(source).unwrap().stored() > Energy::ZERO);
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::ZERO
    );

    let constrain = |state: &AppState| {
        let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, state)).unwrap();
        encoded["state"]["systems"]["energy"]["revision"] = serde_json::json!(u64::MAX - 1);
        serde_json::from_value::<LoadedSaveEnvelope>(encoded)
            .unwrap()
            .into_state(&registries)
            .unwrap()
    };
    unreserved = constrain(&unreserved);
    let mut reserved = constrain(&state);
    assert!(unreserved.can_spend_energy_revisions(1));
    assert!(reserved.can_spend_energy_revisions(0));
    assert!(!reserved.can_spend_energy_revisions(1));

    let unreserved_tick = advance_tick(&registries, &mut unreserved).unwrap();
    assert_eq!(unreserved_tick.mechanical_transmissions().len(), 1);
    assert_eq!(unreserved.energy().revision(), u64::MAX);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &unreserved),
        Ok(())
    );

    // With two revisions available, one shaft transfer can settle while the
    // remaining revision stays reserved for the later manual-power completion.
    let mut sufficient_save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    sufficient_save["state"]["systems"]["energy"]["revision"] = serde_json::json!(u64::MAX - 2);
    let mut sufficient = serde_json::from_value::<LoadedSaveEnvelope>(sufficient_save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let replay_bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &sufficient)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&replay_bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert!(sufficient.can_spend_energy_revisions(1));
    let expected_tick = advance_tick(&registries, &mut sufficient).unwrap();
    assert_eq!(
        expected_tick,
        advance_tick(&registries, &mut replay).unwrap()
    );
    assert_eq!(expected_tick.mechanical_transmissions().len(), 1);
    assert_eq!(sufficient, replay);
    assert_eq!(sufficient.energy().revision(), u64::MAX - 1);
    assert_eq!(sufficient.checked_future_energy_revision_demand(), Some(1));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &sufficient),
        Ok(())
    );

    // With only the promised revision left, the shaft waits without
    // consuming work, energy or a revision. The pending manual-power job
    // must still advance and complete, including across a save/load replay.
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &reserved)).unwrap();
    let mut reserved_replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let energy_before = reserved.energy().get_store(source).unwrap().stored();
    let first = advance_tick(&registries, &mut reserved).unwrap();
    assert_eq!(
        first,
        advance_tick(&registries, &mut reserved_replay).unwrap()
    );
    assert!(first.mechanical_transmissions().is_empty());
    assert_eq!(reserved.energy().revision(), u64::MAX - 1);
    assert_eq!(
        reserved.energy().get_store(source).unwrap().stored(),
        energy_before
    );
    assert_eq!(reserved.checked_future_energy_revision_demand(), Some(1));
    assert_eq!(reserved, reserved_replay);

    while reserved.player_work().active().is_some() {
        let tick = advance_tick(&registries, &mut reserved).unwrap();
        assert_eq!(
            tick,
            advance_tick(&registries, &mut reserved_replay).unwrap()
        );
        assert!(tick.mechanical_transmissions().is_empty());
    }
    assert_eq!(reserved, reserved_replay);
    assert_eq!(reserved.energy().revision(), u64::MAX);
    assert!(reserved.energy().get_store(work_store).unwrap().stored() > Energy::ZERO);
    assert_eq!(reserved.checked_future_energy_revision_demand(), Some(0));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &reserved),
        Ok(())
    );
}

#[test]
fn built_roller_fills_nearly_full_drive_with_friction_adjusted_custody_and_replay() {
    let authored = build_registries();
    let source_capacity = authored
        .energy()
        .get_store(ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap()
        .capacity();
    let destination_capacity = authored
        .energy()
        .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
        .unwrap()
        .capacity();
    // The small drive loses a larger share of its capacity than the large
    // destination gains. Leave precisely enough headroom to transmit a
    // two-nanojoule draw with one-nanojoule delivery without reversing the
    // normalized charge gradient.
    let near_full_gap = crate::core::arithmetic::checked_mul_div_ceil(
        destination_capacity.nanojoules(),
        2,
        source_capacity.nanojoules(),
    )
    .unwrap()
    .checked_add(1)
    .unwrap();
    let (registries, mut state, source, destination, roller_cell, _) =
        test_worksite_with_initial_energy(
            false,
            2,
            Some((
                source_capacity,
                Energy::from_nanojoules(destination_capacity.nanojoules() - near_full_gap),
            )),
        );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let before_matter = calculate_matter_accounting(&state).unwrap().total();
    let roller = build_roller_at(&registries, &mut state, roller_cell);
    assert!(state.structures().get_element(roller).is_some());
    assert_eq!(
        state.energy().get_store(source).unwrap().stored(),
        source_capacity
    );
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::from_nanojoules(destination_capacity.nanojoules() - near_full_gap)
    );
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    let mut settled = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let original = advance_control_tick(&registries, &mut state).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(original, restored);
        if let Some(tick) = original.simulation() {
            assert_eq!(tick.mechanical_transmissions().len(), 1);
            settled = tick.mechanical_transmissions().first().copied();
        }
    }
    let transfer = settled.expect("completed shaft should deliver into exact remaining headroom");
    assert_eq!(transfer.coupling(), roller);
    assert_eq!(transfer.source(), source);
    assert_eq!(transfer.destination(), destination);
    assert_eq!(transfer.drawn(), Energy::from_nanojoules(2));
    assert_eq!(transfer.delivered(), Energy::from_nanojoules(1));
    assert_eq!(transfer.lost(), Energy::from_nanojoules(1));
    assert_eq!(
        state.energy().get_store(source).unwrap().stored(),
        Energy::from_nanojoules(source_capacity.nanojoules() - 2)
    );
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::from_nanojoules(destination_capacity.nanojoules() - near_full_gap + 1)
    );
    assert_eq!(state, replay);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_matter
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
