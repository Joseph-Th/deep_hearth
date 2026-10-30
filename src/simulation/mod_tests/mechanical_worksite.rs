//! Material-built workshop power path through ordinary pointed construction.

#[path = "mechanical_worksite_real.rs"]
mod real;

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    EQUIPMENT_STONE_HAND_CRANK, FORM_HANDLE, MANUAL_POWER_HAND_CRANK,
    STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
};
use crate::core::quantity::Mass;
use crate::core::quantity::{Area, Length};
use crate::energy::{
    EnergyStoreId, add_energy_store_with_initial_for_fixture, validate_assemble_energy_store,
};
use crate::equipment::{EquipmentDefinitionId, EquipmentId, validate_assemble_equipment};
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, project_material_stacks,
};
use crate::labor::{ManualPowerRequest, PlayerWork, validate_start_manual_power};
use crate::logistics::{
    PlayerActiveSelection, place_energy_store_for_fixture, validate_set_active_selection,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::VoxelBounds;
use crate::structural::{
    GroundedStructuralElementPlan, StructuralElementGeometry, StructuralLifecycle,
    validate_start_player_grounded_structural_construction_from_stack,
};

fn assemble_from_materials(
    registries: &Registries,
    state: &mut AppState,
    definition: crate::energy::EnergyStoreDefinitionId,
) -> EnergyStoreId {
    let profile = registries
        .energy()
        .get_store(definition)
        .unwrap()
        .assembly_profile()
        .unwrap();
    let stockpile = add_solid_stockpile_for_test(state, profile.input_mass()).unwrap();
    for input in profile.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            stockpile,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    validate_assemble_energy_store(registries, state, definition, stockpile)
        .unwrap()
        .commit(state)
        .unwrap()
}

fn assemble_crank(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let definition: EquipmentDefinitionId = EQUIPMENT_STONE_HAND_CRANK;
    let profile = registries
        .equipment()
        .get_equipment(definition)
        .unwrap()
        .assembly_profile()
        .unwrap();
    let stockpile = add_solid_stockpile_for_test(state, profile.input_mass()).unwrap();
    for input in profile.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            stockpile,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    validate_assemble_equipment(registries, state, definition, stockpile)
        .unwrap()
        .commit(state)
        .unwrap()
}

fn test_worksite(
    real_flywheels: bool,
) -> (
    Registries,
    AppState,
    EnergyStoreId,
    EnergyStoreId,
    VoxelCoord,
    Option<EquipmentId>,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x5152_4D45),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(1, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let player_y = surface_y + 1;
    let source_position = VoxelCoord::new(0, player_y, 2);
    let dest_position = VoxelCoord::new(2, player_y, 2);
    let roller_position = VoxelCoord::new(1, player_y, 2);
    for x in 0..=2 {
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
    let (source, destination, crank) = if real_flywheels {
        (
            assemble_from_materials(&registries, &mut state, ENERGY_STONE_FLYWHEEL_DRIVE),
            assemble_from_materials(&registries, &mut state, ENERGY_STONE_FLYWHEEL_DRIVE),
            Some(assemble_crank(&registries, &mut state)),
        )
    } else {
        (
            add_energy_store_with_initial_for_fixture(
                &registries,
                &mut state,
                ENERGY_MECHANICAL_SMALL_DRIVE,
                Energy::from_nanojoules(100_000_000_000),
            )
            .unwrap(),
            add_energy_store_with_initial_for_fixture(
                &registries,
                &mut state,
                ENERGY_MECHANICAL_LARGE_DRIVE,
                Energy::ZERO,
            )
            .unwrap(),
            None,
        )
    };
    place_energy_store_for_fixture(&mut state, source, source_position);
    place_energy_store_for_fixture(&mut state, destination, dest_position);
    if let Some(crank) = crank {
        crate::logistics::place_equipment_for_fixture(&mut state, crank, source_position);
    }
    let spawn = if real_flywheels {
        source_position
    } else {
        VoxelCoord::new(1, player_y, 0)
    };
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (
        registries,
        state,
        source,
        destination,
        roller_position,
        crank,
    )
}

#[test]
fn ordinary_crafted_roller_really_transmits_limited_work_with_friction() {
    let (registries, mut state, source, destination, position, _crank) = test_worksite(false);
    let handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        handle,
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(handle)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let direction = ViewDirection::new(0, -600_000, 800_000).unwrap();
    let before_matter = calculate_matter_accounting(&state).unwrap().total();
    let first = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        first.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let work = match state.player_work().active() {
        Some(PlayerWork::StructuralConstruction { work }) => work,
        other => panic!("held handle did not start physical roller construction: {other:?}"),
    };
    while state.tick() < work.completes_at() {
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(&registries, &mut state).unwrap();
        }
    }
    let roller = state
        .structures()
        .elements()
        .find(|member| member.profile() == STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING)
        .expect("a completed authored roller should exist");
    assert_eq!(roller.lifecycle(), StructuralLifecycle::Active);
    assert_eq!(roller.bounds().min(), position);
    assert_eq!(roller.embodied_mass(), Mass::from_milligrams(195_000));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_matter
    );

    let source_before = state.energy().get_store(source).unwrap().stored();
    let dest_before = state.energy().get_store(destination).unwrap().stored();
    let mut receipts = Vec::new();
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let step = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(slow) = step.simulation() {
            receipts.extend_from_slice(slow.mechanical_transmissions());
        }
    }
    assert_eq!(
        receipts.len(),
        1,
        "built roller should move stored work autonomously"
    );
    let transfer = receipts[0];
    assert_eq!(transfer.source(), source);
    assert_eq!(transfer.destination(), destination);
    assert!(
        !transfer.lost().is_zero(),
        "wood friction must lose real work"
    );
    assert_eq!(
        source_before.checked_sub(state.energy().get_store(source).unwrap().stored()),
        Some(transfer.drawn()),
    );
    assert_eq!(
        state
            .energy()
            .get_store(destination)
            .unwrap()
            .stored()
            .checked_sub(dest_before),
        Some(transfer.delivered()),
    );
    assert!(transfer.drawn().nanojoules() <= 180_000_000_000);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state,
    );
}
