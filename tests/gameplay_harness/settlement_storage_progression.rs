//! Settlement-scale finite-work storage progression and orchestration witness.

use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_HELVE_HAMMER, EQUIPMENT_TIMBER_SPINDLE_DRILL,
    PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING, PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
    PROCESS_POWER_GRIND_STONE_SCRAP_TOOL, PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
    PROCESS_POWER_TURN_TIMBER_FLYWHEEL, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{
    EnergyStoreId, validate_assemble_energy_store, validate_upgrade_energy_store,
};
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::maintenance::Condition;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

use super::powered_craft_planning::{authored_batch, project_powered_craft_sequence};

const PORTFOLIO_BATCHES_PER_FAMILY: u64 = 32;

#[derive(Clone, Copy)]
struct PoweredFamily {
    process: ProcessId,
    equipment: EquipmentDefinitionId,
    label: &'static str,
}

const POWERED_FAMILIES: [PoweredFamily; 5] = [
    PoweredFamily {
        process: PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
        equipment: EQUIPMENT_TIMBER_SPINDLE_DRILL,
        label: "spindle-drill",
    },
    PoweredFamily {
        process: PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
        equipment: EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
        label: "wire-drawbench",
    },
    PoweredFamily {
        process: PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
        equipment: EQUIPMENT_TIMBER_HELVE_HAMMER,
        label: "helve-hammer",
    },
    PoweredFamily {
        process: PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        equipment: EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        label: "flywheel-lathe",
    },
    PoweredFamily {
        process: PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        equipment: EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        label: "flywheel-grindstone",
    },
];

#[derive(Clone, Copy)]
struct PortfolioCadence {
    charge_events: u64,
    largest_packed_leg: u64,
}

fn project_portfolio_cadence(
    registries: &Registries,
    state: &AppState,
    drive: EnergyStoreId,
    stage: &'static str,
) -> PortfolioCadence {
    let mut charge_events = 0_u64;
    let mut largest_packed_leg = 0_u64;
    for family in POWERED_FAMILIES {
        let batch = authored_batch(registries, family.process, family.label);
        let sequence = project_powered_craft_sequence(
            registries,
            state,
            family.process,
            family.equipment,
            Condition::PRISTINE,
            drive,
            batch,
            PORTFOLIO_BATCHES_PER_FAMILY,
            family.label,
        );
        assert_eq!(
            sequence.batches, PORTFOLIO_BATCHES_PER_FAMILY,
            "{stage} storage must keep the declared {} workload feasible",
            family.label
        );
        charge_events = charge_events
            .checked_add(sequence.charge_events())
            .unwrap_or_else(|| panic!("{stage} storage portfolio charge count overflowed"));
        largest_packed_leg = largest_packed_leg.max(sequence.maximum_leg_batches());
    }
    PortfolioCadence {
        charge_events,
        largest_packed_leg,
    }
}

pub(super) fn run_settlement_storage_progression_experience() {
    let registries = build_registries();
    let mut state = AppState::new();
    let package = super::settlement_fixture::seed_energy_upgrade_package(
        &registries,
        &mut state,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        &[
            ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
            ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        ],
        "settlement storage progression",
    );
    super::world_admission::admit_stationary_player(
        &registries,
        &mut state,
        &[package],
        &[],
        "settlement storage progression",
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("settlement storage initial matter audit failed: {error}"))
        .total();

    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, package)
            .unwrap_or_else(|error| panic!("settlement storage base assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("settlement storage base assembly commit failed: {error}")
            });
    let stone_state = state.clone();
    let stone_mass = stone_state
        .energy()
        .get_store(drive)
        .map(|record| record.embodied_mass())
        .unwrap_or_else(|| panic!("settlement storage base drive disappeared"));

    validate_upgrade_energy_store(
        &registries,
        &state,
        drive,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
        package,
    )
    .unwrap_or_else(|error| panic!("settlement storage paired upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement storage paired upgrade commit failed: {error}"));
    let paired_state = state.clone();
    let paired_mass = paired_state
        .energy()
        .get_store(drive)
        .map(|record| record.embodied_mass())
        .unwrap_or_else(|| panic!("settlement storage paired drive disappeared"));

    validate_upgrade_energy_store(
        &registries,
        &state,
        drive,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        package,
    )
    .unwrap_or_else(|error| panic!("settlement storage bank upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement storage bank upgrade commit failed: {error}"));
    let bank_mass = state
        .energy()
        .get_store(drive)
        .map(|record| record.embodied_mass())
        .unwrap_or_else(|| panic!("settlement storage workshop bank disappeared"));
    assert!(stone_mass < paired_mass && paired_mass < bank_mass);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(package)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "settlement storage ladder must consume exactly its disclosed additive material"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("settlement storage matter audit failed: {error}"))
            .total(),
        matter_before,
        "settlement storage scaling must conserve embodied matter"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("settlement storage final state invalid: {error}"));

    let stone = project_portfolio_cadence(&registries, &stone_state, drive, "stone");
    let paired = project_portfolio_cadence(&registries, &paired_state, drive, "paired");
    let bank = project_portfolio_cadence(&registries, &state, drive, "bank");
    assert!(paired.charge_events < stone.charge_events);
    assert!(bank.charge_events < paired.charge_events);

    let capacity = |definition| {
        registries
            .energy()
            .get_store(definition)
            .map(|record| record.capacity().nanojoules())
            .unwrap_or_else(|| panic!("settlement storage definition disappeared"))
    };
    reviewln!(
        "SETTLEMENT STORAGE EXPERIENCE identity-preserved=true additive=true shaped-input-scope=storage-body-only stages=[stone:[capacity:{}nJ embodied:{}mg portfolio-charges:{} max-packed:{}] paired:[capacity:{}nJ embodied:{}mg portfolio-charges:{} max-packed:{}] bank:[capacity:{}nJ embodied:{}mg portfolio-charges:{} max-packed:{}]] portfolio=[families:{} batches-per-family:{} productive-batches:{}] matter=conserved",
        capacity(ENERGY_STONE_FLYWHEEL_DRIVE),
        stone_mass.milligrams(),
        stone.charge_events,
        stone.largest_packed_leg,
        capacity(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE),
        paired_mass.milligrams(),
        paired.charge_events,
        paired.largest_packed_leg,
        capacity(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK),
        bank_mass.milligrams(),
        bank.charge_events,
        bank.largest_packed_leg,
        POWERED_FAMILIES.len(),
        PORTFOLIO_BATCHES_PER_FAMILY,
        PORTFOLIO_BATCHES_PER_FAMILY * POWERED_FAMILIES.len() as u64,
    );
}
