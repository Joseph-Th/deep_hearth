//! Ordinary pump-drill to spindle-drill investment contract.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_SPINDLE_DRILL, FORM_REINFORCEMENT, FORM_SCRAP,
    FORM_SCREEN_PLATE, MATERIAL_COPPER, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_PIERCE_COPPER_SCREEN_PLATE, PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
    PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::resolve_manual_craft;
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;

use super::capital_investment_crossover::first_attention_return_crossover;
use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::powered_craft_planning::authored_batch;
use super::settlement_generation::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, crossover_workloads,
};
use super::settlement_workshop_investment::{
    PoweredProjectPlan, execute_powered_project, project_powered_project,
};

const SPINDLE_DEMAND_SALT: u64 = 0x5350_494E_444C_4544;

fn seed_material(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) {
    let _ = seed_lot(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        ROOM_TEMPERATURE,
    );
}

pub(super) fn run_spindle_drill_investment_experience(variation_root: Option<u64>) {
    let registries = build_registries();
    let spindle_batch = authored_batch(
        &registries,
        PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
        "spindle-drill investment",
    );
    let mut state = AppState::new();

    // This is a later settlement workshop: the large flywheel bank was earned by prior power
    // infrastructure progression, while the drill upgrade additions below are still fabricated
    // through ordinary play and remain the investment decision under test.
    let bootstrap = super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[
            EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
            EQUIPMENT_STONE_HAND_CRANK,
        ],
        &[ENERGY_TIMBER_FRAME_FLYWHEEL_BANK],
        &[],
        "spindle-drill prior workshop",
    );

    // Wear the portable drill once before the decision so the upgrade has to preserve real use,
    // not merely a pristine definition identity.
    let calibration_source = seed_stockpile(
        &mut state,
        spindle_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        calibration_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        spindle_batch.input_mass,
    );
    let calibration_output = seed_stockpile(
        &mut state,
        spindle_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let (upgrade_raw, upgrade_parts) = super::settlement_workshop_investment::seed_upgrade_raw(
        &registries,
        &mut state,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        "spindle-drill investment",
    );
    let work_mass = Mass::from_milligrams(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES
            .checked_mul(spindle_batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("spindle disclosed input opportunity overflowed")),
    );
    let work_source = seed_stockpile(
        &mut state,
        work_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        work_mass,
    );
    let baseline_output = seed_stockpile(
        &mut state,
        work_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let powered_output = seed_stockpile(
        &mut state,
        work_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );

    super::world_admission::admit_stationary_player(
        &registries,
        &mut state,
        &[
            bootstrap,
            calibration_source,
            calibration_output,
            upgrade_raw,
            upgrade_parts,
            work_source,
            baseline_output,
            powered_output,
        ],
        &[],
        "spindle-drill investment",
    );
    let pump = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("pump drill assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pump drill assembly commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("spindle investment crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("spindle investment crank commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("spindle investment workshop-bank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("spindle investment workshop-bank commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    let calibration = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_PIERCE_COPPER_SCREEN_PLATE,
        calibration_source,
        1,
        "spindle investment pump-drill calibration",
    )
    .with_equipment(pump);
    let _ = execute_manual_craft(
        &registries,
        &mut state,
        calibration,
        calibration_output,
        "spindle investment pump-drill calibration",
    );
    let pump_condition = state
        .equipment()
        .get_equipment(pump)
        .unwrap_or_else(|| panic!("worn pump drill disappeared"))
        .condition();
    assert!(pump_condition < deep_hearth::maintenance::Condition::PRISTINE);

    let board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        upgrade_raw,
        2,
        "spindle frame boards",
    );
    let handle_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        upgrade_raw,
        2,
        "spindle frame handles",
    );
    let copper_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        upgrade_raw,
        1,
        "spindle bearing reinforcement",
    );
    let setup_attention = resolve_manual_craft(&registries, &state, &board_request)
        .unwrap_or_else(|error| panic!("spindle board projection failed: {error}"))
        .duration()
        .value()
        + resolve_manual_craft(&registries, &state, &handle_request)
            .unwrap_or_else(|error| panic!("spindle handle projection failed: {error}"))
            .duration()
            .value()
        + resolve_manual_craft(&registries, &state, &copper_request)
            .unwrap_or_else(|error| panic!("spindle copper projection failed: {error}"))
            .duration()
            .value();
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("spindle hand crank disappeared before investment decision"));
    let policy = CapitalInvestmentPolicy::baseline();
    let minimum_attention_return = policy.minimum_attention_return(0, setup_attention);
    let crossover = first_attention_return_crossover(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        minimum_attention_return,
        |batches| {
            let request = select_manual_craft_request(
                &registries,
                &state,
                PROCESS_PIERCE_COPPER_SCREEN_PLATE,
                work_source,
                batches,
                "spindle crossover pump baseline",
            )
            .with_equipment(pump);
            let baseline = resolve_manual_craft(&registries, &state, &request).ok()?;
            let powered = project_powered_project(
                &registries,
                &state,
                PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
                EQUIPMENT_TIMBER_SPINDLE_DRILL,
                pump_condition,
                crank_condition,
                drive,
                spindle_batch,
                batches,
                "spindle crossover charging",
            )?;
            Some((
                baseline.duration().value(),
                setup_attention
                    .checked_add(powered.charging.attention_ticks)
                    .unwrap_or_else(|| panic!("spindle crossover attention overflowed")),
            ))
        },
    )
    .unwrap_or_else(|| {
        panic!("spindle drill has no attention-return crossover in the disclosed opportunity")
    });
    let workloads = crossover_workloads(
        crossover,
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        variation_root.map(|root| root ^ SPINDLE_DEMAND_SALT),
    );
    let short_order = workloads.short_batches;
    let project_order = workloads.project_batches;
    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            work_source,
            short_order,
            "spindle short-order pump baseline",
        )
        .with_equipment(pump),
    )
    .unwrap_or_else(|error| panic!("spindle short baseline projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_PIERCE_COPPER_SCREEN_PLATE,
        work_source,
        project_order,
        "spindle project pump baseline",
    )
    .with_equipment(pump);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("spindle project baseline projection failed: {error}"));
    let short_powered_projection = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        pump_condition,
        crank_condition,
        drive,
        spindle_batch,
        short_order,
        "spindle short-order charging",
    )
    .unwrap_or_else(|| panic!("spindle short-order powered route became unavailable"));
    let project_powered_projection = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        pump_condition,
        crank_condition,
        drive,
        spindle_batch,
        project_order,
        "spindle project charging",
    )
    .unwrap_or_else(|| panic!("spindle project powered route became unavailable"));
    let short_machine_attention =
        setup_attention + short_powered_projection.charging.attention_ticks;
    assert!(
        !clears_attention_return(
            short_baseline.duration().value(),
            short_machine_attention,
            minimum_attention_return,
        ),
        "a short screen-plate order should keep using the already-owned pump drill"
    );
    let project_machine_attention =
        setup_attention + project_powered_projection.charging.attention_ticks;
    assert!(
        clears_attention_return(
            project_baseline.duration().value(),
            project_machine_attention,
            minimum_attention_return,
        ),
        "repeated screen-plate work should repay the spindle frame by the shared capital-return floor"
    );

    let decision_state = state.clone();
    let matter_before = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("spindle investment matter setup failed: {error}"))
        .total();
    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "spindle project pump baseline",
    );

    let mut powered = decision_state;
    let mut executed_setup = 0_u64;
    for (process, batches, context) in [
        (PROCESS_SHAPE_WOOD_BOARDS, 2, "spindle frame boards"),
        (PROCESS_SHAPE_WOOD_HANDLE, 2, "spindle frame handles"),
        (
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            1,
            "spindle bearing reinforcement",
        ),
    ] {
        let request = select_manual_craft_request(
            &registries,
            &powered,
            process,
            upgrade_raw,
            batches,
            context,
        );
        executed_setup +=
            execute_manual_craft(&registries, &mut powered, request, upgrade_parts, context)
                .value();
    }
    assert_eq!(executed_setup, setup_attention);
    let spindle = validate_upgrade_equipment(
        &registries,
        &powered,
        pump,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("pump-to-spindle upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("pump-to-spindle upgrade commit failed: {error}"));
    assert_eq!(spindle, pump);
    assert_eq!(
        powered
            .equipment()
            .get_equipment(spindle)
            .map(|record| record.condition()),
        Some(pump_condition),
        "spindle conversion must retain the worn pump drill's condition"
    );

    let powered_project = execute_powered_project(
        &registries,
        &mut powered,
        PoweredProjectPlan {
            process: PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
            source: work_source,
            destination: powered_output,
            machine: spindle,
            crank,
            drive,
            batch: spindle_batch,
            batches: project_order,
            context: "spindle project unattended drilling",
        },
    );
    assert_eq!(
        powered_project.charge_attention,
        project_powered_projection.charging.attention_ticks
    );
    assert_eq!(
        powered_project.delegated_ticks,
        project_powered_projection.sequence.delegated_ticks
    );
    assert_eq!(
        powered_project.charge_events,
        project_powered_projection.sequence.charge_events()
    );
    assert_eq!(
        powered
            .equipment()
            .get_equipment(crank)
            .map(|record| record.condition()),
        Some(project_powered_projection.charging.condition_after),
        "spindle projected hand-crank wear must match execution"
    );
    assert_eq!(
        executed_setup + powered_project.charge_attention,
        project_machine_attention
    );
    assert!(project_machine_attention < baseline_ticks.value());
    for commodity in [
        CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
    ] {
        assert_eq!(
            powered
                .inventory()
                .get_stockpile(powered_output)
                .map(|stockpile| stockpile.get_mass(commodity)),
            baseline
                .inventory()
                .get_stockpile(baseline_output)
                .map(|stockpile| stockpile.get_mass(commodity)),
            "powered spindle drilling must preserve the manual pump drill's exact material transform"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("spindle baseline matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("spindle investment matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("spindle investment final state invalid: {error}"));
    reviewln!(
        "SETTLEMENT MACHINE EXPERIENCE family=spindle-drill transform=screen-plate prior=pump-drill upgrade=spindle-drill policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t crossover:{}batches short=[batches:{} baseline:{}t machine:{}t choice:keep-prior] project=[batches:{} baseline:{}t setup:{}t charging:{}t machine:{}t attention-saved:{}t delegated:{}t power-cycle=[charges:{} max-batches-per-charge:{}] choice:upgrade] used-identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved",
        policy.minimum_return_ppm(),
        minimum_attention_return,
        crossover,
        short_order,
        short_baseline.duration().value(),
        short_machine_attention,
        project_order,
        project_baseline.duration().value(),
        setup_attention,
        project_powered_projection.charging.attention_ticks,
        project_machine_attention,
        project_baseline.duration().value() - project_machine_attention,
        powered_project.delegated_ticks,
        powered_project.charge_events,
        powered_project.maximum_leg_batches,
    );
}

#[test]
fn spindle_drill_converts_used_pump_drill_when_repeated_plate_work_repays_attention() {
    run_spindle_drill_investment_experience(None);
}
