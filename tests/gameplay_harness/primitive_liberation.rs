//! Ordinary primitive liberation episode shared by progression gates and exploratory reports.

use deep_hearth::content::gameplay_fixture::{seed_composed_lot, seed_lot};
use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_STONE_SEPARATOR,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, EQUIPMENT_TIMBER_TREADLE_DRIVE, FORM_SCRAP,
    FORM_SCREEN_PLATE, MATERIAL_COPPER, PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_PIERCE_COPPER_SCREEN_PLATE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::equipment::{EquipmentId, validate_upgrade_equipment};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::resolve_representable_screening_mass;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::ore_fixture::copper_ore_composition;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;

#[path = "primitive_liberation/comparison.rs"]
mod comparison;
#[path = "primitive_liberation/primary.rs"]
mod primary;
#[path = "primitive_liberation/scavenging.rs"]
mod scavenging;
#[path = "primitive_liberation/support.rs"]
mod support;

#[derive(Clone)]
struct PrimitiveLiberationScenario {
    charge_policy: support::ChargePolicy,
    charges: Vec<support::ChargeReport>,
    state: AppState,
    batch_mass: Mass,
    copper_ppm: u32,
    ore: deep_hearth::inventory::StockpileId,
    crushed: deep_hearth::inventory::StockpileId,
    ground: deep_hearth::inventory::StockpileId,
    undersize: deep_hearth::inventory::StockpileId,
    oversize: deep_hearth::inventory::StockpileId,
    concentrate: deep_hearth::inventory::StockpileId,
    tailings: deep_hearth::inventory::StockpileId,
    fine_tailings: deep_hearth::inventory::StockpileId,
    exhausted_tailings: deep_hearth::inventory::StockpileId,
    ore_lot: deep_hearth::inventory::MaterialLotId,
    crusher: EquipmentId,
    quern: EquipmentId,
    screen: EquipmentId,
    separator: EquipmentId,
    treadle: EquipmentId,
    drive: EnergyStoreId,
}

pub(super) fn run_primitive_liberation_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let requested_batch_mass =
        Mass::from_milligrams(80_000 + mix64(seed ^ 0x4C49_4245_5241_5445) % 40_001);
    let grinding = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation grinding definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation screening definition disappeared"));
    let batch_mass = resolve_representable_screening_mass(
        screening,
        grinding.output_particle_size_distribution(),
        requested_batch_mass,
    )
    .unwrap_or_else(|error| panic!("primitive liberation batch planning failed: {error}"));
    assert!(
        !batch_mass.is_zero(),
        "primitive liberation generated no representable batch"
    );
    let copper_ppm = 300_000 + (mix64(seed ^ 0x4C49_4245_5243_5550) % 300_001) as u32;
    let clay_share_ppm = (mix64(seed ^ 0x4C49_4245_5243_4C41) % 650_001) as u32;
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x51A2_1B3A_7100_0001));
    let ore = add_solid_stockpile(&mut state, batch_mass);
    let crushed = add_solid_stockpile(&mut state, batch_mass);
    let ground = add_solid_stockpile(&mut state, batch_mass);
    let undersize = add_solid_stockpile(&mut state, batch_mass);
    let oversize = add_solid_stockpile(&mut state, batch_mass);
    let concentrate = add_solid_stockpile(&mut state, batch_mass);
    let tailings = add_solid_stockpile(&mut state, batch_mass);
    let fine_tailings = add_solid_stockpile(&mut state, batch_mass);
    let exhausted_tailings = add_solid_stockpile(&mut state, batch_mass);
    let ore_lot = seed_composed_lot(
        registries,
        &mut state,
        ore,
        CommodityKey::new(MATERIAL_COPPER, deep_hearth::content::FORM_ORE),
        batch_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(copper_ppm, clay_share_ppm),
    );
    let crusher = support::assemble_equipment_from_authored_parts(
        registries,
        &mut state,
        EQUIPMENT_STONE_CRUSHER,
    );
    let quern = support::assemble_equipment_from_authored_parts(
        registries,
        &mut state,
        EQUIPMENT_STONE_ROTARY_QUERN,
    );
    let screen = support::assemble_equipment_from_authored_parts(
        registries,
        &mut state,
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
    );
    let plate_craft = registries
        .crafting()
        .get_manual(PROCESS_PIERCE_COPPER_SCREEN_PLATE)
        .unwrap_or_else(|| panic!("primitive sizing-plate craft definition disappeared"));
    let screen_plate = plate_craft
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE))
        .unwrap_or_else(|| panic!("primitive sizing-plate craft lost its screen-plate output"));
    let screen_upgrade_material = add_solid_stockpile(&mut state, plate_craft.input_mass());
    let screen_plate_source = add_solid_stockpile(&mut state, plate_craft.input_mass());
    let screen_plate_input = seed_lot(
        registries,
        &mut state,
        screen_plate_source,
        plate_craft.input(),
        plate_craft.input_mass(),
        ROOM_TEMPERATURE,
    );
    let separator = support::assemble_equipment_from_authored_parts(
        registries,
        &mut state,
        EQUIPMENT_STONE_SEPARATOR,
    );
    let treadle = support::assemble_equipment_from_authored_parts(
        registries,
        &mut state,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
    );
    let drive = support::assemble_energy_store_from_authored_parts(
        registries,
        &mut state,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
    );
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("paired primitive drive disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("primitive liberation matter setup failed: {error}"))
        .total();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive liberation survival setup failed: {error}"));
    let plate_job = validate_start_manual_craft(
        registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            screen_plate_source,
            MaterialLotSelection::new(screen_plate_input, plate_craft.input_mass()),
            screen_upgrade_material,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive sizing-plate craft failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive sizing-plate craft commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        plate_job,
        "primitive copper sizing plate",
    );
    let authored_scrap = plate_craft
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP))
        .map(|output| output.mass())
        .unwrap_or(Mass::ZERO);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(screen_upgrade_material)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP))
            }),
        Some(authored_scrap),
        "piercing the sizing plate must retain its authored reworkable byproduct"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(screen_upgrade_material)
            .map(|stockpile| stockpile.get_mass(screen_plate.commodity())),
        Some(screen_plate.mass()),
        "the authored plate output must become the exact additive screen-upgrade stock"
    );
    let upgraded_screen = validate_upgrade_equipment(
        registries,
        &state,
        screen,
        EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
        screen_upgrade_material,
    )
    .unwrap_or_else(|error| panic!("primitive sizing-screen upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive sizing-screen upgrade commit failed: {error}"));
    assert_eq!(
        upgraded_screen, screen,
        "copper sizing plate must upgrade the existing timber riddle in place"
    );

    let mut scenario = PrimitiveLiberationScenario {
        charge_policy: support::ChargePolicy::BatchDemand,
        charges: Vec::new(),
        state,
        batch_mass,
        copper_ppm,
        ore,
        crushed,
        ground,
        undersize,
        oversize,
        concentrate,
        tailings,
        fine_tailings,
        exhausted_tailings,
        ore_lot,
        crusher,
        quern,
        screen,
        separator,
        treadle,
        drive,
    };
    let mut full_buffer = scenario.clone();
    full_buffer.charge_policy = support::ChargePolicy::FullBuffer;
    let started_at = scenario.state.tick().value();
    let primary = primary::run(registries, &mut scenario);
    let primary_completed_at = scenario.state.tick().value();
    let scavenged = scavenging::run(registries, &mut scenario, &primary);
    let baseline_primary = primary::run(registries, &mut full_buffer);
    let baseline_scavenged = scavenging::run(registries, &mut full_buffer, &baseline_primary);
    assert_eq!(
        primary, baseline_primary,
        "charging policy must preserve primary recovery"
    );
    assert_eq!(
        scavenged, baseline_scavenged,
        "charging policy must preserve scavenger recovery"
    );
    comparison::review(
        registries,
        seed,
        started_at,
        primary_completed_at,
        &scenario,
        &full_buffer,
        &scavenged,
    );
    let state = &scenario.state;
    assert!(
        state
            .energy()
            .get_store(drive)
            .is_some_and(|record| record.stored() < drive_capacity)
    );
    for equipment in [crusher, quern, screen, separator] {
        assert!(
            state
                .equipment()
                .get_equipment(equipment)
                .is_some_and(|record| record.condition() < Condition::PRISTINE),
            "every primitive liberation machine must incur real operation wear"
        );
    }
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!("primitive liberation matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("primitive liberation trusted-state audit failed: {error}"));
    let final_energy = state
        .energy()
        .get_store(drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("primitive liberation drive disappeared after completion"));
    let recovered_copper_ppm_mg = primary
        .concentrate_copper_ppm_mg
        .checked_add(scavenged.additional_recovered_copper_ppm_mg)
        .unwrap_or_else(|| panic!("primitive liberation recovered-copper audit overflowed"));
    // Use exact constituent numerators, not rounded concentrate grades. Keep any fractional
    // milligrams as decimal digits without floating point or truncating represented copper.
    let copper_mg = |numerator: u128| {
        let whole = numerator.checked_div(1_000_000).expect("nonzero ppm scale");
        let fraction = numerator.checked_rem(1_000_000).expect("nonzero ppm scale");
        if fraction == 0 {
            whole.to_string()
        } else {
            let digits = format!("{fraction:06}");
            format!("{whole}.{}", digits.trim_end_matches('0'))
        }
    };
    reviewln!(
        "LIBERATION EXPERIENCE seed=0x{seed:016X} sample={} route=treadle+paired-flywheel->crusher->quern->copper-screen->regrind->separator->tailings-regrind->scavenger input=[{}mg {}ppm-Cu clay-share:{}ppm] concentrate=[first:{}mg/{}ppm final:{}mg/{}ppm] copper-in-concentrate=[first:{}mg final:{}mg scavenger-recovered:{}mg] exhausted-tailings={}mg stored-work-remaining={}nJ machinery-worn=true matter=conserved",
        focused_probe_role_label(case.role()),
        batch_mass.milligrams(),
        copper_ppm,
        clay_share_ppm,
        primary.concentrate_mass.milligrams(),
        primary.concentrate_grade_ppm,
        scavenged.concentrate_mass.milligrams(),
        scavenged.concentrate_grade_ppm,
        copper_mg(primary.concentrate_copper_ppm_mg),
        copper_mg(recovered_copper_ppm_mg),
        copper_mg(scavenged.additional_recovered_copper_ppm_mg),
        scavenged.exhausted_tailings_mass.milligrams(),
        final_energy.nanojoules(),
    );
    // The concentrate stockpile has no ordinary sink yet: reduction/smelting into pure metal is
    // still the STATUS.md frontier, so the scavenger leg reads as future-proofing rather than
    // immediate copper. Report its share of recovered copper alongside that boundary.
    let scavenger_share_ppm = if recovered_copper_ppm_mg == 0 {
        0
    } else {
        (scavenged
            .additional_recovered_copper_ppm_mg
            .checked_mul(1_000_000)
            .unwrap_or_else(|| panic!("primitive liberation scavenger-share audit overflowed"))
            / recovered_copper_ppm_mg)
            .min(1_000_000)
    };
    reviewln!(
        "LIBERATION FRONTIER seed=0x{seed:016X} sample={} input=[{}mg {}ppm-Cu] concentrate=[final:{}mg/{}ppm] scavenger=[extra-copper:{}mg share:{}ppm-of-recovered-copper] sink=none-ordinary smelting-frontier=prepared-ore-concentrate->pure-metal reachability-authority=STATUS.md",
        focused_probe_role_label(case.role()),
        batch_mass.milligrams(),
        copper_ppm,
        scavenged.concentrate_mass.milligrams(),
        scavenged.concentrate_grade_ppm,
        scavenged.additional_recovered_copper_ppm_mg / 1_000_000,
        scavenger_share_ppm,
    );
}
