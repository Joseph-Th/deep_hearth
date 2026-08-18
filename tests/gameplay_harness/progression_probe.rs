//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use std::num::NonZeroU64;

use super::{ROOM_TEMPERATURE, add_solid_stockpile, seed_lot};
use deep_hearth::content::gameplay_fixture::seed_geological_deposit;
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_STONE_PICK, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    MINING_METHOD_HAND_PICK, PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CRUSH_ORE,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::{
    ManualCraftRequest, ManualCraftStartRequest, validate_start_manual_craft,
};
use deep_hearth::energy::{calculate_mass_specific_energy, validate_assemble_energy_store};
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::{CommodityKey, CompositionComponent, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{validate_claim_mining_output, validate_start_mining};
use deep_hearth::ore_processing::{ComminutionRequest, resolve_comminution_process};
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{assess_survival, initialize_player_survival};

fn advance_exact(registries: &Registries, state: &mut AppState, ticks: u64) {
    for _ in 0..ticks {
        advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("primitive progression tick failed: {error}"));
    }
}

fn duration(start: u64, end: u64) -> u64 {
    end.checked_sub(start)
        .unwrap_or_else(|| panic!("primitive progression work duration underflowed"))
}

fn craft_batches(
    registries: &Registries,
    state: &mut AppState,
    process: deep_hearth::production::ProcessId,
    source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    batches: u64,
    authored_ticks_per_batch: u64,
) {
    let batches = NonZeroU64::new(batches)
        .unwrap_or_else(|| panic!("primitive progression craft batch count must be nonzero"));
    validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(
            ManualCraftRequest::new(process, source, batches),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive progression repeated craft failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression repeated craft commit failed: {error}"));
    advance_exact(
        registries,
        state,
        authored_ticks_per_batch
            .checked_mul(batches.get())
            .unwrap_or_else(|| panic!("primitive progression repeated craft duration overflowed")),
    );
}

fn mine_and_claim(
    registries: &Registries,
    state: &mut AppState,
    deposit: deep_hearth::geology::GeologicalDepositId,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> u64 {
    let mining = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        equipment,
        mass,
    )
    .unwrap_or_else(|error| panic!("primitive progression mining failed: {error}"));
    let mining_job = mining
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression mining commit failed: {error}"));
    let mining_record = state
        .mining()
        .get_job(mining_job)
        .unwrap_or_else(|| panic!("primitive progression mining job disappeared"));
    let mining_ticks = duration(
        mining_record.started_at().value(),
        mining_record.completes_at().value(),
    );
    advance_exact(registries, state, mining_ticks);
    validate_claim_mining_output(registries, state, mining_job)
        .unwrap_or_else(|error| panic!("primitive progression mining claim failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("primitive progression mining claim commit failed: {error}")
        });
    mining_ticks
}

pub(super) fn run_primitive_progression_probe(registries: &Registries, seed: u64) {
    const MINED_MASS: Mass = Mass::from_milligrams(200_000);
    const ORE_TOTAL: Mass = Mass::from_milligrams(400_000);
    const NATIVE_COPPER_TOTAL: Mass = Mass::from_milligrams(40_000);

    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive progression survival setup failed: {error}"));
    let raw = add_solid_stockpile(
        &mut state,
        Mass::from_milligrams(12_000_000),
        "primitive raw materials",
    );
    let shaped = add_solid_stockpile(
        &mut state,
        Mass::from_milligrams(12_000_000),
        "primitive shaped materials",
    );
    let ore_storage = add_solid_stockpile(&mut state, ORE_TOTAL, "primitive mined ore");
    let native_storage =
        add_solid_stockpile(&mut state, NATIVE_COPPER_TOTAL, "primitive native copper");
    let crushed_storage = add_solid_stockpile(&mut state, MINED_MASS, "primitive crushed ore");
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(6_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(6_000_000),
        ROOM_TEMPERATURE,
    );
    let ore_bounds = VoxelBounds::new(VoxelCoord::new(0, -4, 0), VoxelCoord::new(1, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression deposit bounds failed: {error}"));
    let ore_composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 700_000),
        CompositionComponent::new(MATERIAL_STONE, 300_000),
    ])
    .unwrap_or_else(|error| panic!("primitive progression ore composition failed: {error}"));
    let ore_deposit = seed_geological_deposit(
        registries,
        &mut state,
        ore_bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        ORE_TOTAL,
        Temperature::from_millikelvin(293_150),
        ore_composition,
    );
    let native_bounds = VoxelBounds::new(VoxelCoord::new(2, -4, 0), VoxelCoord::new(3, -3, 1))
        .unwrap_or_else(|error| panic!("primitive native-copper bounds failed: {error}"));
    let native_deposit = seed_geological_deposit(
        registries,
        &mut state,
        native_bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        NATIVE_COPPER_TOTAL,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_COPPER),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("primitive progression initial matter audit failed: {error}")
        })
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression survival state disappeared"));

    validate_start_manual_craft(
        registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, raw, shaped),
    )
    .unwrap_or_else(|error| panic!("primitive progression knapping failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive progression knapping commit failed: {error}"));
    advance_exact(registries, &mut state, 40);
    validate_start_manual_craft(
        registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_SHAPE_WOOD_HANDLE, raw, shaped),
    )
    .unwrap_or_else(|error| panic!("primitive progression first handle failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive progression first handle commit failed: {error}"));
    advance_exact(registries, &mut state, 40);
    let pick = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_PICK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression pick assembly commit failed: {error}")
        });

    let stone_mining_ticks = mine_and_claim(
        registries,
        &mut state,
        ore_deposit,
        ore_storage,
        pick,
        MINED_MASS,
    );
    let native_mining_ticks = mine_and_claim(
        registries,
        &mut state,
        native_deposit,
        native_storage,
        pick,
        NATIVE_COPPER_TOTAL,
    );
    let worn_stone_condition = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("primitive progression worn pick disappeared"))
        .condition();

    craft_batches(
        registries,
        &mut state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        native_storage,
        shaped,
        2,
        40,
    );
    validate_upgrade_equipment(
        registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive progression pick reinforcement failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("primitive progression pick reinforcement commit failed: {error}")
    });
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("primitive progression reinforced pick disappeared"))
            .condition(),
        worn_stone_condition,
        "reinforcement must not repair accumulated pick wear"
    );
    let reinforced_mining_ticks = mine_and_claim(
        registries,
        &mut state,
        ore_deposit,
        ore_storage,
        pick,
        MINED_MASS,
    );
    assert!(
        reinforced_mining_ticks < stone_mining_ticks,
        "copper reinforcement should reduce active extraction time for the same mass"
    );

    craft_batches(
        registries,
        &mut state,
        PROCESS_SHAPE_STONE_FLYWHEEL,
        raw,
        shaped,
        2,
        60,
    );
    craft_batches(
        registries,
        &mut state,
        PROCESS_SHAPE_WOOD_HANDLE,
        raw,
        shaped,
        2,
        40,
    );
    let crank = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_HAND_CRANK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression crank assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression crank assembly commit failed: {error}")
        });

    let drive =
        validate_assemble_energy_store(registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, shaped)
            .unwrap_or_else(|error| {
                panic!("primitive progression drive construction failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("primitive progression drive construction commit failed: {error}")
            });

    let crusher_process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive progression crusher process disappeared"));
    let required_energy =
        calculate_mass_specific_energy(MINED_MASS, crusher_process.specific_energy());
    let stone_power = validate_start_manual_power(
        registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, required_energy),
    )
    .unwrap_or_else(|error| panic!("primitive progression stone-crank projection failed: {error}"));
    let stone_charge_ticks = duration(
        stone_power.work().started_at().value(),
        stone_power.work().completes_at().value(),
    );
    validate_upgrade_equipment(
        registries,
        &state,
        crank,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive progression crank reinforcement failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("primitive progression crank reinforcement commit failed: {error}")
    });

    craft_batches(
        registries,
        &mut state,
        PROCESS_KNAP_STONE_TOOL,
        raw,
        shaped,
        3,
        40,
    );
    craft_batches(
        registries,
        &mut state,
        PROCESS_SHAPE_WOOD_HANDLE,
        raw,
        shaped,
        3,
        40,
    );
    let crusher = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_CRUSHER, shaped)
        .unwrap_or_else(|error| {
            panic!("primitive progression crusher construction failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression crusher construction commit failed: {error}")
        });
    let power = validate_start_manual_power(
        registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, required_energy),
    )
    .unwrap_or_else(|error| panic!("primitive progression manual charging failed: {error}"));
    let charge_work = power.work();
    let charge_ticks = duration(
        charge_work.started_at().value(),
        charge_work.completes_at().value(),
    );
    power
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("primitive progression charge commit failed: {error}"));
    advance_exact(registries, &mut state, charge_ticks);
    assert_eq!(
        stone_charge_ticks,
        charge_ticks * 2,
        "copper reinforcement should halve primitive charging time without changing stored work"
    );

    let ore_lot = state
        .inventory()
        .lot_ids(ore_storage)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.mass() >= MINED_MASS)
        })
        .unwrap_or_else(|| panic!("primitive progression claimed ore lot disappeared"));
    let selection = [MaterialLotSelection::new(ore_lot, MINED_MASS)];
    let resolved = resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(PROCESS_CRUSH_ORE, ore_storage, &selection, crusher, drive),
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), required_energy);
    let crush_ticks = resolved.process_resolution().duration().value();
    validate_start_process(
        registries,
        &state,
        resolved.process_resolution(),
        ore_storage,
        crushed_storage,
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive progression crushing commit failed: {error}"));
    advance_exact(registries, &mut state, crush_ticks);

    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression final survival state disappeared"));
    assert!(survival_after.metabolic_energy() < survival_before.metabolic_energy());
    assert!(survival_after.hydration() < survival_before.hydration());
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "primitive progression final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(crushed_storage)
            .unwrap_or_else(|| panic!("primitive progression crushed storage disappeared"))
            .stored_mass(),
        MINED_MASS
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("primitive progression persistence audit failed: {error}"));

    let drive_mass = state
        .energy()
        .get_store(drive)
        .unwrap_or_else(|| panic!("primitive progression constructed drive disappeared"))
        .embodied_mass();
    let crusher_mass = state
        .equipment()
        .get_equipment(crusher)
        .unwrap_or_else(|| panic!("primitive progression constructed crusher disappeared"))
        .embodied_mass();

    std::println!(
        "PROGRESSION fantasy=survive->craft-tools->extract-ore->find-native-metal->reinforce-tools->extract-better->build-power->reinforce-power->build-machine->mechanize ore={}mg+{}mg native={}mg mining=[stone-ore:{}t native:{}t reinforced-ore:{}t] infrastructure=[drive:{}mg crusher:{}mg] charge={}nJ/[stone:{}t reinforced:{}t reduction:{}x] crush={}t stored-work-burst={}x survival=[energy:-{}nJ hydration:-{}uL] matter=conserved",
        MINED_MASS.milligrams(),
        MINED_MASS.milligrams(),
        NATIVE_COPPER_TOTAL.milligrams(),
        stone_mining_ticks,
        native_mining_ticks,
        reinforced_mining_ticks,
        drive_mass.milligrams(),
        crusher_mass.milligrams(),
        required_energy.nanojoules(),
        stone_charge_ticks,
        charge_ticks,
        stone_charge_ticks / charge_ticks.max(1),
        crush_ticks,
        charge_ticks / crush_ticks.max(1),
        survival_before.metabolic_energy().nanojoules()
            - survival_after.metabolic_energy().nanojoules(),
        survival_before.hydration().microliters() - survival_after.hydration().microliters(),
    );
}
