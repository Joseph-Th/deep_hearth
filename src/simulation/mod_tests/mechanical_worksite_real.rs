//! A real crank charges one assembled flywheel through a built wooden roller.

use super::*;

#[test]
fn player_assembled_flywheels_charge_a_remote_drive_through_real_timber() {
    let (registries, mut state, source, destination, middle, crank) = test_worksite(true);
    let crank = crank.expect("physical worksite should own the assembled hand crank");
    assert_eq!(
        state.energy().get_store(source).unwrap().stored(),
        Energy::ZERO
    );
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::ZERO
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    // Player effort charges the local drive only; the distant machine remains unpowered.
    let requested = Energy::from_nanojoules(400_000_000_000);
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, source, requested),
    )
    .expect("assembled crank should charge its exact-local flywheel")
    .commit(&mut state)
    .unwrap();
    while state.player_work().active().is_some() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let step = advance_control_tick(&registries, &mut state).unwrap();
            if let Some(slow) = step.simulation() {
                assert!(slow.mechanical_transmissions().is_empty());
            }
        }
    }
    assert!(state.energy().get_store(source).unwrap().stored() > Energy::ZERO);
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::ZERO
    );

    // This is not a free menu unlock. A whole crafted handle enters exact
    // inventory custody; 195 g is embodied into a supported shaft and 5 g
    // remains as usable material in the carried stack.
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        handle,
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == handle)
        .unwrap();
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(
            middle,
            VoxelCoord::new(middle.x() + 1, middle.y() + 1, middle.z() + 1),
        )
        .unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(300),
    )
    .unwrap();
    let initial_mass = calculate_matter_accounting(&state).unwrap().total();
    let start = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
            MATERIAL_WOOD,
            geometry,
        ),
        &stack,
    )
    .expect("a crafted handle should become a real ground-supported roller");
    let roller = start.element();
    let work = start.commit(&mut state).unwrap();
    while state.tick() < work.completes_at() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(&registries, &mut state).unwrap();
        }
    }
    assert_eq!(
        state.structures().get_element(roller).unwrap().lifecycle(),
        StructuralLifecycle::Active,
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_mass
    );
    assert!(state.energy().get_store(source).unwrap().stored() > Energy::ZERO);
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::ZERO
    );

    // The second accumulator charges while the player is idle, using only
    // finite stored mechanical work previously produced by the player.
    let mut transferred = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let tick = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(slow) = tick.simulation() {
            transferred = slow.mechanical_transmissions().first().copied();
        }
    }
    let transfer = transferred.expect("completed roller should transmit mechanical power");
    assert_eq!(transfer.coupling(), roller);
    assert_eq!(transfer.source(), source);
    assert_eq!(transfer.destination(), destination);
    assert_eq!(
        transfer.delivered().nanojoules(),
        transfer.drawn().nanojoules() * 4 / 5,
    );
    assert!(state.energy().get_store(destination).unwrap().stored() > Energy::ZERO);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_mass
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    // Deconstruction pays back conserved timber and terminates the electrical-
    // free mechanical route, rather than leaving an invisible permanent bus.
    let dismantle = crate::structural::validate_start_player_structural_dismantling(
        &registries,
        &state,
        roller,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .expect("the player must be able to dismantle an accessible shaft")
    .commit(&mut state)
    .unwrap();
    while state.tick() < dismantle.completes_at() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let step = advance_control_tick(&registries, &mut state).unwrap();
            if let Some(slow) = step.simulation()
                && slow.tick() == dismantle.completes_at()
            {
                assert!(
                    slow.mechanical_transmissions().is_empty(),
                    "removed roller must disconnect before same-tick power settlement"
                );
            }
        }
    }
    assert!(state.structures().get_element(roller).is_none());
    let recovered = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .get_mass(handle);
    assert!(recovered.milligrams() >= 195_000);
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let tick = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(slow) = tick.simulation() {
            assert!(slow.mechanical_transmissions().is_empty());
        }
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_mass
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
