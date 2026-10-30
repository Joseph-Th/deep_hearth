//! Workshop continuation: a player charges, builds and uses a remote ore crusher.

use super::*;
use crate::content::{
    BLOCK_STATE_GRADED_GALENA_ORE, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_PICK, FORM_CHIP,
    FORM_CRUSHED, FORM_CRUSHER_BLOCK, FORM_CRUSHER_JAW_FACE, FORM_LOG, FORM_LUMP, FORM_ORE,
    FORM_TOOL, MATERIAL_GALENA, MATERIAL_STONE, MINING_METHOD_HAND_PICK, PROCESS_CRUSH_ORE,
    PROCESS_KNAP_STONE_CRUSHER_JAW_FACE, PROCESS_KNAP_STONE_TOOL,
    PROCESS_SHAPE_STONE_CRUSHER_BLOCK, PROCESS_SHAPE_WOOD_HANDLE,
};
use crate::core::quantity::Mass;
use crate::crafting::{player_manual_craft_options, validate_start_player_manual_craft};
use crate::logistics::{
    HorizontalControlInput, PlayerControlInput, validate_allocate_player_ground_stockpile,
    validate_drop_stack_to_ground,
};
use crate::ore_processing::{
    PlayerComminutionOptionError, player_local_powered_comminution_options,
    validate_start_player_comminution_option,
};
use crate::world::{
    BlockExcavationOutputReceipt, BlockExcavationRequest, validate_start_player_block_excavation,
};
use std::num::NonZeroU64;

fn craft_player_workshop_part(
    registries: &Registries,
    state: &mut AppState,
    process: crate::production::ProcessId,
    batches: u64,
) {
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(registries, state).unwrap();
    }
    let catalog = player_manual_craft_options(registries, state).unwrap();
    assert!(catalog.iter().any(|option| option.process() == process));
    let count = NonZeroU64::new(batches).expect("fabrication batch must be nonzero");
    let job = validate_start_player_manual_craft(registries, state, process, count, None)
        .expect("raw stock should support player hand fabrication")
        .commit(state)
        .unwrap();
    let complete = state.production().get_job(job).unwrap().completes_at();
    while state.tick() < complete {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(registries, state).unwrap();
        }
    }
    assert!(state.player_work().active().is_none());
}

#[test]
fn player_runs_unattended_crusher_at_the_far_end_of_a_material_built_power_link() {
    let (registries, mut state, source, destination, middle, crank) =
        test_worksite_with_distance(true, 4);
    let crank = crank.unwrap();
    let start_tick = state.tick();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    // Only raw stone and logs enter the player's carried custody. Every
    // tool, crusher part, and chip comes from normal hand crafting.
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(3_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(6_000_000),
        ),
    ] {
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
    let ore = CommodityKey::new(MATERIAL_GALENA, FORM_ORE);
    let grade = registries
        .world()
        .block_for_state(BLOCK_STATE_GRADED_GALENA_ORE)
        .unwrap()
        .excavation()
        .unwrap()
        .composition()
        .clone();
    let before_shaping = calculate_matter_accounting(&state).unwrap().total();
    craft_player_workshop_part(
        &registries,
        &mut state,
        PROCESS_SHAPE_STONE_CRUSHER_BLOCK,
        1,
    );
    craft_player_workshop_part(
        &registries,
        &mut state,
        PROCESS_KNAP_STONE_CRUSHER_JAW_FACE,
        1,
    );
    craft_player_workshop_part(&registries, &mut state, PROCESS_KNAP_STONE_TOOL, 1);
    craft_player_workshop_part(&registries, &mut state, PROCESS_SHAPE_WOOD_HANDLE, 6);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_shaping
    );
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHER_BLOCK),
            Mass::from_milligrams(1_200_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHER_JAW_FACE),
            Mass::from_milligrams(400_000),
        ),
        (handle, Mass::from_milligrams(1_200_000)),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(600_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            Mass::from_milligrams(4_800_000),
        ),
    ] {
        assert_eq!(
            state
                .inventory()
                .get_stockpile(carried)
                .unwrap()
                .get_mass(commodity),
            mass
        );
    }
    // The three-roller shaft dissipates 48.8% of transmitted work, so the
    // two batches require more up-front crank work than a one-roller drive.
    // Charge only AFTER 360 ticks of fabrication to avoid flywheel windage.
    // The remote drive remains empty until an actual shaft is constructed.
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            source,
            Energy::from_nanojoules(500_000_000_000),
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
    // rollers; later their completed collision bodies obstruct this shortcut.
    for x in 0..=6 {
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
    for _ in 0..210 {
        if state.logistics().player().unwrap().position().x() >= 4 {
            break;
        }
        let _ = advance_control_tick_with_input(&registries, &mut state, forward).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 4);
    for _ in 0..20 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 4);

    // Prepared parts came from actual hand-fabrication jobs at the source.
    // The crusher consumes 400 g of handles; the stone pick another 200 g,
    // leaving an entire roller blank and spare handle stock.
    let crusher =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_CRUSHER, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(
        state.logistics().stationary_equipment_position(crusher),
        Some(VoxelCoord::new(4, player_y, 2)),
    );
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state.logistics().stationary_equipment_position(pick),
        Some(VoxelCoord::new(4, player_y, 2))
    );

    // An exposed grade-bearing mineral cell is authoritative world matter.
    // The ordinary hand-pick path mines exactly 100 g into carried custody.
    let target = VoxelCoord::new(4, player_y, 3);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target,
        BLOCK_STATE_GRADED_GALENA_ORE,
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let direction = ViewDirection::new(0, -800_000, 600_000).unwrap();
    let source_mass_before = state
        .world()
        .represented_mass_at(registries.world(), target)
        .unwrap();
    let mining_matter_before = calculate_matter_accounting(&state).unwrap().total();
    let pick_condition_before = state.equipment().get_equipment(pick).unwrap().condition();
    let mining = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(
            direction,
            MINING_METHOD_HAND_PICK,
            pick,
            Mass::from_milligrams(100_000),
        ),
    )
    .expect("physical exposed galena should admit hand-pick extraction")
    .commit(&mut state)
    .unwrap();
    let mut mined = None;
    while state.control_tick() < mining.completes_at() {
        let step = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(receipt) = step.block_excavation() {
            mined = Some(receipt);
        }
    }
    let BlockExcavationOutputReceipt::Carried { lot: ore_lot } = mined
        .expect("grade-bearing seam must produce an actual material lot")
        .output()
    else {
        panic!("a 100 g ore batch must fit carried capacity")
    };
    assert_eq!(
        state.inventory().get_lot(ore_lot).unwrap().composition(),
        &grade
    );
    assert_eq!(
        state.inventory().get_lot(ore_lot).unwrap().mass(),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        source_mass_before.checked_sub(Mass::from_milligrams(100_000))
    );
    assert!(state.equipment().get_equipment(pick).unwrap().condition() < pick_condition_before);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        mining_matter_before
    );
    // The still-solid cell represents the remaining deposit, not a magically
    // empty voxel after the player removes only 100 g.
    assert_eq!(
        state.world().block_at(target),
        Some(BLOCK_STATE_GRADED_GALENA_ORE)
    );

    // The three finite ground bays are ordinarily allocated by the player.
    // Mined ore is deposited as one exact grade-preserving stack.
    let feed = validate_allocate_player_ground_stockpile(&state, Mass::from_milligrams(150_000))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let out = validate_allocate_player_ground_stockpile(&state, Mass::from_milligrams(65_000))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let second_out =
        validate_allocate_player_ground_stockpile(&state, Mass::from_milligrams(150_000))
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == ore)
        .unwrap();
    assert_eq!(stack.composition(), &grade);
    validate_drop_stack_to_ground(
        &registries,
        &state,
        feed,
        &stack,
        Mass::from_milligrams(100_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(ore),
        Mass::ZERO
    );
    assert_eq!(
        state.inventory().get_stockpile(feed).unwrap().get_mass(ore),
        Mass::from_milligrams(100_000)
    );

    // Three 195 g rollers are independently embodied from the remaining
    // wooden handle stock. No single-cell virtual link can reach this crusher.
    let before = calculate_matter_accounting(&state).unwrap().total();
    let mut rollers = Vec::new();
    for x in 1..=3 {
        rollers.push(super::shaft::build_roller_at(
            &registries,
            &mut state,
            VoxelCoord::new(x, middle.y(), middle.z()),
        ));
    }
    assert_eq!(rollers.len(), 3);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(handle),
        Mass::from_milligrams(15_000),
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    let mut received = false;
    for _ in 0..12 {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let tick = advance_control_tick(&registries, &mut state).unwrap();
            if let Some(slow) = tick.simulation()
                && slow.mechanical_transmissions().iter().any(|transfer| {
                    transfer.source() == source
                        && transfer.destination() == destination
                        && transfer.segments() == 3
                        && transfer.coupling() == rollers[0]
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
        if state.logistics().player().unwrap().position().x() >= 5 {
            break;
        }
        let _ = advance_control_tick_with_input(&registries, &mut state, forward).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 5);
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
        product.get_mass(CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)),
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
        if state.logistics().player().unwrap().position().x() <= 4 {
            break;
        }
        let _ = advance_control_tick_with_input(&registries, &mut state, back).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position().x(), 4);
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
            .get_mass(CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)),
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
