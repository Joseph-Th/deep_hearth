//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use super::{ROOM_TEMPERATURE, add_solid_stockpile, seed_lot};
use deep_hearth::content::gameplay_fixture::seed_geological_deposit;
use deep_hearth::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, EQUIPMENT_JAW_CRUSHER, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_STONE_PICK, FORM_LOG, FORM_LUMP, FORM_ORE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, MINING_METHOD_HAND_PICK, PROCESS_CRUSH_ORE,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use deep_hearth::energy::{add_energy_store, calculate_mass_specific_energy};
use deep_hearth::equipment::{add_equipment, validate_assemble_equipment};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{CommodityKey, MaterialComposition};
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

pub(super) fn run_primitive_progression_probe(registries: &Registries, seed: u64) {
    const MINED_MASS: Mass = Mass::from_milligrams(200_000);

    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive progression survival setup failed: {error}"));
    let raw = add_solid_stockpile(
        &mut state,
        Mass::from_milligrams(4_000_000),
        "primitive raw materials",
    );
    let shaped = add_solid_stockpile(
        &mut state,
        Mass::from_milligrams(4_000_000),
        "primitive shaped materials",
    );
    let ore_storage = add_solid_stockpile(&mut state, MINED_MASS, "primitive mined ore");
    let crushed_storage = add_solid_stockpile(&mut state, MINED_MASS, "primitive crushed ore");
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(2_000_000),
        ROOM_TEMPERATURE,
    );
    let bounds = VoxelBounds::new(VoxelCoord::new(0, -4, 0), VoxelCoord::new(1, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression deposit bounds failed: {error}"));
    let deposit = seed_geological_deposit(
        registries,
        &mut state,
        bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        MINED_MASS,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_COPPER),
    );
    let crusher = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("primitive progression crusher fixture failed: {error}"));
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

    let mining = validate_start_mining(
        registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        ore_storage,
        pick,
        MINED_MASS,
    )
    .unwrap_or_else(|error| panic!("primitive progression mining failed: {error}"));
    let mining_job = mining
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("primitive progression mining commit failed: {error}"));
    let mining_record = state
        .mining()
        .get_job(mining_job)
        .unwrap_or_else(|| panic!("primitive progression mining job disappeared"));
    let mining_ticks = duration(
        mining_record.started_at().value(),
        mining_record.completes_at().value(),
    );
    advance_exact(registries, &mut state, mining_ticks);
    validate_claim_mining_output(registries, &state, mining_job)
        .unwrap_or_else(|error| panic!("primitive progression mining claim failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression mining claim commit failed: {error}")
        });

    validate_start_manual_craft(
        registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_SHAPE_STONE_FLYWHEEL, raw, shaped),
    )
    .unwrap_or_else(|error| panic!("primitive progression flywheel shaping failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive progression flywheel commit failed: {error}"));
    advance_exact(registries, &mut state, 60);
    validate_start_manual_craft(
        registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_SHAPE_WOOD_HANDLE, raw, shaped),
    )
    .unwrap_or_else(|error| panic!("primitive progression second handle failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive progression second handle commit failed: {error}"));
    advance_exact(registries, &mut state, 40);
    let crank = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_HAND_CRANK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression crank assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression crank assembly commit failed: {error}")
        });

    let drive = add_energy_store(registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("primitive progression drive allocation failed: {error}"));
    let crusher_process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive progression crusher process disappeared"));
    let required_energy =
        calculate_mass_specific_energy(MINED_MASS, crusher_process.specific_energy());
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

    let ore_lot = state
        .inventory()
        .lot_ids(ore_storage)
        .next()
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

    std::println!(
        "PROGRESSION fantasy=survive->craft->extract->store-work->mechanize mined={}mg mining={}t charge={}nJ/{}t crush={}t leverage={}x survival=[energy:-{}nJ hydration:-{}uL] matter=conserved",
        MINED_MASS.milligrams(),
        mining_ticks,
        required_energy.nanojoules(),
        charge_ticks,
        crush_ticks,
        charge_ticks / crush_ticks.max(1),
        survival_before.metabolic_energy().nanojoules()
            - survival_after.metabolic_energy().nanojoules(),
        survival_before.hydration().microliters() - survival_after.hydration().microliters(),
    );
}
