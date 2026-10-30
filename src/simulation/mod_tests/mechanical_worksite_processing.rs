//! Workshop continuation: a player charges, builds and uses a remote ore crusher.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_CRUSHER, FORM_CRUSHED, FORM_CRUSHER_BLOCK, FORM_CRUSHER_JAW_FACE, FORM_ORE,
    MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_STONE, PROCESS_CRUSH_ORE,
};
use crate::core::quantity::Mass;
use crate::inventory::add_solid_stockpile_for_test;
use crate::logistics::{
    HorizontalControlInput, PlayerControlInput, validate_place_ground_stockpile,
};
use crate::material::{CompositionComponent, MaterialComposition};
use crate::ore_processing::{
    PlayerComminutionOptionError, player_local_powered_comminution_options,
    validate_start_player_comminution_option,
};

#[test]
fn player_runs_unattended_crusher_at_the_far_end_of_a_material_built_power_link() {
    let (registries, mut state, source, destination, middle, crank) = test_worksite(true);
    let crank = crank.unwrap();
    let start_tick = state.tick();

    // Charge the player-assembled local stone flywheel by doing physical
    // manual work. The remote drive remains empty until a shaft exists.
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            source,
            Energy::from_nanojoules(400_000_000_000),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    while state.player_work().active().is_some() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(&registries, &mut state).unwrap();
        }
    }

    // There is no production at an unpowered, distant worksite.
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::ZERO
    );
    assert!(
        player_local_powered_comminution_options(&registries, &state)
            .unwrap()
            .is_empty()
    );

    let player_y = middle.y();
    // Establish a real walkable line to the far worksite before building the
    // roller; later its completed collision body will obstruct this shortcut.
    for x in 0..=4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, player_y - 1, 2),
            BLOCK_STATE_STONE,
        );
        for y in player_y..=player_y + 2 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 2),
                BlockStateId::AIR,
            );
        }
    }
    let forward = PlayerControlInput::new(HorizontalControlInput::new(1_000_000, 0).unwrap());
    for _ in 0..70 {
        if state.logistics().player().unwrap().position().x() >= 2 {
            break;
        }
        let _ = advance_control_tick_with_input(&registries, &mut state, forward).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 2);
    for _ in 0..20 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 2);

    let carried = state.logistics().player().unwrap().carried_stockpile();
    let handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    let assembly_inputs = [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHER_BLOCK),
            Mass::from_milligrams(1_200_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHER_JAW_FACE),
            Mass::from_milligrams(400_000),
        ),
        (handle, Mass::from_milligrams(600_000)),
    ];
    for (commodity, mass) in assembly_inputs {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    // Real 2 kg crusher assembly withdraws precise prepared parts from
    // carried custody and locates the durable machine at the player's voxel.
    let crusher =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_CRUSHER, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(
        state.logistics().stationary_equipment_position(crusher),
        Some(VoxelCoord::new(2, player_y, 2)),
    );

    // Supply one graded feed and two finite destinations. The first output
    // bay only accepts 65 g, so a 100 g ore charge needs two real batches.
    let feed = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(150_000)).unwrap();
    let out = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(65_000)).unwrap();
    let second_out =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(150_000)).unwrap();
    let grade = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 400_000),
        CompositionComponent::new(MATERIAL_SLAG, 600_000),
    ])
    .unwrap();
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    crate::inventory::deposit_composed_lot_for_test(
        &registries,
        &mut state,
        feed,
        ore,
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
        grade.clone(),
    )
    .unwrap();
    for stockpile in [feed, out, second_out] {
        validate_place_ground_stockpile(&state, stockpile, VoxelCoord::new(2, player_y, 2))
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }

    // Physical construction is admitted only on a settled slow boundary.
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    // Physically embody a 195 g wooden coupling between the two power
    // stores. It is not a button that teleports the source's whole charge.
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == handle)
        .unwrap();
    let bounds = VoxelBounds::new(
        middle,
        VoxelCoord::new(middle.x() + 1, middle.y() + 1, middle.z() + 1),
    )
    .unwrap();
    let geometry = StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(300),
    )
    .unwrap();
    let before = calculate_matter_accounting(&state).unwrap().total();
    let build = validate_start_player_grounded_structural_construction_from_stack(
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
    .unwrap()
    .commit(&mut state)
    .unwrap();
    while state.tick() < build.completes_at() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(&registries, &mut state).unwrap();
        }
    }
    let mut received = false;
    for _ in 0..12 {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let tick = advance_control_tick(&registries, &mut state).unwrap();
            if let Some(slow) = tick.simulation()
                && slow.mechanical_transmissions().iter().any(|transfer| {
                    transfer.source() == source && transfer.destination() == destination
                })
            {
                received = true;
            }
        }
        if state.energy().get_store(destination).unwrap().stored()
            >= Energy::from_nanojoules(100_000_000_000)
        {
            break;
        }
    }
    assert!(
        received,
        "the mechanically coupled worksite should accumulate usable work"
    );

    // The ready menu uses the actual ore, material grade, charged flywheel,
    // assembled crusher and chosen output bin. It requires no lot-ID guesses.
    let options = player_local_powered_comminution_options(&registries, &state).unwrap();
    let choice = options
        .iter()
        .find(|option| {
            option.source() == feed
                && option.destination() == out
                && option.equipment() == crusher
                && option.energy_store() == destination
                && option.process() == PROCESS_CRUSH_ORE
        })
        .expect("the assembled and powered stone crusher should be operable from the local menu");
    assert_eq!(
        choice.maximum_mass(),
        Mass::from_milligrams(65_000),
        "output-bay headroom must cap real batch size"
    );
    assert_eq!(
        choice.required_energy_for_maximum(),
        Energy::from_nanojoules(65_000_000_000)
    );
    let duration = choice.duration_for_maximum();
    assert!(
        duration.value() >= 10,
        "ore must retain its authored real crushing duration"
    );
    let job = validate_start_player_comminution_option(
        &registries,
        &state,
        choice,
        choice.maximum_mass(),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(state.production().get_job(job).is_some());
    assert!(state.tick() > start_tick);
    assert!(
        matches!(
            validate_start_player_comminution_option(
                &registries,
                &state,
                choice,
                choice.maximum_mass(),
            ),
            Err(PlayerComminutionOptionError::Stack(
                crate::inventory::MaterialStackProjectionError::StaleInventoryRevision { .. }
            ))
        ),
        "a spent menu snapshot must not select a different inventory cohort"
    );

    // Inputs and energy are reserved once. Leave the stocked machine in place
    // and walk another real cell; neither feed nor product follows the player.
    for _ in 0..70 {
        if state.logistics().player().unwrap().position().x() >= 3 {
            break;
        }
        let _ = advance_control_tick_with_input(&registries, &mut state, forward).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 3);
    // Authoritative save during live off-site crushing must replay the same
    // job completion, energy history, ore particle state, and inventory owner.
    let paused = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&paused)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert!(state.production().get_job(job).is_some());
    for _ in 0..(duration.value() + 2) * CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
        let _ = advance_control_tick(&registries, &mut resumed).unwrap();
    }
    assert_eq!(
        state, resumed,
        "off-site crushing must replay exactly after a save"
    );
    let product = state.inventory().get_stockpile(out).unwrap();
    assert_eq!(
        product.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)),
        Mass::from_milligrams(65_000),
    );
    assert_eq!(
        state.inventory().get_stockpile(feed).unwrap().get_mass(ore),
        Mass::from_milligrams(35_000),
    );
    let output_lot_id = state.inventory().lot_ids(out).next().unwrap();
    let output_lot = state.inventory().get_lot(output_lot_id).unwrap();
    assert_eq!(output_lot.composition(), &grade);
    assert!(output_lot.particle_size().is_some());

    // Return to the workshop and finish the remaining 35 g in a separate
    // bin. The wooden coupling automatically refills the idle flywheel after
    // the first process releases it. No new work is generated for free.
    let back = PlayerControlInput::new(HorizontalControlInput::new(-1_000_000, 0).unwrap());
    for _ in 0..70 {
        if state.logistics().player().unwrap().position().x() <= 2 {
            break;
        }
        let _ = advance_control_tick_with_input(&registries, &mut state, back).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 2);
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK * 3 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    // At 20 Hz the worksite remains visible as a deferred preview, rather
    // than flashing an empty menu for 71 out of every 72 control frames.
    assert!(!state.control_tick().is_simulation_boundary());
    let preview = player_local_powered_comminution_options(&registries, &state).unwrap();
    let pending = preview
        .iter()
        .find(|option| {
            option.source() == feed
                && option.destination() == second_out
                && option.energy_store() == destination
        })
        .expect("feasible machine must remain visible between simulation boundaries");
    assert!(pending.deferred_until_boundary());
    let unchanged = state.clone();
    assert!(matches!(
        validate_start_player_comminution_option(
            &registries,
            &state,
            pending,
            pending.maximum_mass(),
        ),
        Err(PlayerComminutionOptionError::Start(
            crate::production::StartPlayerProcessError::Process(
                crate::production::StartProcessError::SimulationBoundary(_)
            )
        )),
    ));
    assert_eq!(
        state, unchanged,
        "preview must not circumvent timed admission"
    );
    // The actual production start waits for the authoritative slow boundary.
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let ready = player_local_powered_comminution_options(&registries, &state).unwrap();
    let next = ready
        .iter()
        .find(|option| {
            option.source() == feed
                && option.destination() == second_out
                && option.equipment() == crusher
                && option.energy_store() == destination
        })
        .expect("the second, smaller batch must fit a separate output bay");
    assert!(!next.deferred_until_boundary());
    assert_eq!(next.maximum_mass(), Mass::from_milligrams(35_000));
    let duration2 = next.duration_for_maximum();
    let _ =
        validate_start_player_comminution_option(&registries, &state, next, next.maximum_mass())
            .unwrap()
            .commit(&mut state)
            .unwrap();
    for _ in 0..(duration2.value() + 1) * CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.inventory().get_stockpile(feed).unwrap().get_mass(ore),
        Mass::ZERO,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(second_out)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)),
        Mass::from_milligrams(35_000),
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state,
    );
}
