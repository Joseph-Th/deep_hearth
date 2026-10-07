//! Ordinary manual-to-powered wire-drawing investment contract.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_WIRE_DRAWBENCH, FORM_ELECTRICAL_WINDING,
    FORM_REINFORCEMENT, MATERIAL_COPPER, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_DRAW_COPPER_ELECTRICAL_WINDING, PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
    PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
    build_registries,
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

const WIRE_DEMAND_SALT: u64 = 0x5749_5245_4445_4D44;

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

pub(super) fn run_wire_drawbench_investment_experience(variation_root: Option<u64>) {
    let registries = build_registries();
    let powered_batch = authored_batch(
        &registries,
        PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
        "flywheel drawbench investment",
    );

    let mut state = AppState::new();

    // The decision begins with the manual lossless drawbench already owned. This later settlement
    // workshop has already scaled its finite-work buffer, so the comparison prices the drawbench
    // automation conversion rather than replaying primitive power infrastructure acquisition.
    let bootstrap = super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[EQUIPMENT_TIMBER_WIRE_DRAWBENCH, EQUIPMENT_STONE_HAND_CRANK],
        &[ENERGY_TIMBER_FRAME_FLYWHEEL_BANK],
        &[],
        "wire-drawbench prior workshop",
    );

    // Put one real batch through the manual bench before the choice. The powered upgrade must
    // preserve that used machine identity and condition rather than replacing it with a pristine
    // parallel object.
    let calibration_source = seed_stockpile(
        &mut state,
        powered_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        calibration_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        powered_batch.input_mass,
    );
    let calibration_output = seed_stockpile(
        &mut state,
        powered_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let (upgrade_raw, upgrade_parts) = super::settlement_workshop_investment::seed_upgrade_raw(
        &registries,
        &mut state,
        EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
        "flywheel drawbench investment",
    );

    let order_mass = Mass::from_milligrams(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES
            .checked_mul(powered_batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("wire disclosed input opportunity overflowed")),
    );
    let work_source = seed_stockpile(
        &mut state,
        order_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        order_mass,
    );
    let baseline_output = seed_stockpile(
        &mut state,
        order_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let powered_output = seed_stockpile(
        &mut state,
        order_mass,
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
        "wire-drawbench investment",
    );
    let drawbench = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_WIRE_DRAWBENCH,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("manual drawbench assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual drawbench assembly commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("wire investment crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("wire investment crank commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("wire investment workshop-bank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wire investment workshop-bank commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    let calibration_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
        calibration_source,
        1,
        "wire drawbench calibration",
    )
    .with_equipment(drawbench);
    let _ = execute_manual_craft(
        &registries,
        &mut state,
        calibration_request,
        calibration_output,
        "wire drawbench calibration",
    );
    let used_condition = state
        .equipment()
        .get_equipment(drawbench)
        .unwrap_or_else(|| panic!("used wire drawbench disappeared"))
        .condition();
    assert!(used_condition < deep_hearth::maintenance::Condition::PRISTINE);

    let flywheel_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_STONE_FLYWHEEL,
        upgrade_raw,
        1,
        "wire automation flywheel",
    );
    let board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        upgrade_raw,
        2,
        "wire automation frame boards",
    );
    let handle_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        upgrade_raw,
        2,
        "wire automation handles",
    );
    let copper_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        upgrade_raw,
        1,
        "wire automation bearing reinforcement",
    );
    let setup_attention = [
        &flywheel_request,
        &board_request,
        &handle_request,
        &copper_request,
    ]
    .into_iter()
    .map(|request| {
        resolve_manual_craft(&registries, &state, request)
            .unwrap_or_else(|error| panic!("wire automation setup projection failed: {error}"))
            .duration()
            .value()
    })
    .try_fold(0_u64, |total, ticks| total.checked_add(ticks))
    .unwrap_or_else(|| panic!("wire automation setup attention overflowed"));

    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("wire hand crank disappeared before investment decision"));
    let policy = CapitalInvestmentPolicy::baseline();
    let minimum_attention_return = policy.minimum_attention_return(0, setup_attention);
    let crossover = first_attention_return_crossover(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        minimum_attention_return,
        |batches| {
            let request = select_manual_craft_request(
                &registries,
                &state,
                PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
                work_source,
                batches,
                "wire crossover manual baseline",
            )
            .with_equipment(drawbench);
            let baseline = resolve_manual_craft(&registries, &state, &request).ok()?;
            let powered = project_powered_project(
                &registries,
                &state,
                PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
                EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
                used_condition,
                crank_condition,
                drive,
                powered_batch,
                batches,
                "wire crossover charging",
            )?;
            Some((
                baseline.duration().value(),
                setup_attention
                    .checked_add(powered.charging.attention_ticks)
                    .unwrap_or_else(|| panic!("wire crossover attention overflowed")),
            ))
        },
    )
    .unwrap_or_else(|| {
        panic!("wire drawbench has no attention-return crossover in the disclosed opportunity")
    });
    let workloads = crossover_workloads(
        crossover,
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        variation_root.map(|root| root ^ WIRE_DEMAND_SALT),
    );
    let short_order = workloads.short_batches;
    let project_order = workloads.project_batches;
    let project_mass = Mass::from_milligrams(
        project_order
            .checked_mul(powered_batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("wire selected project input mass overflowed")),
    );
    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
            work_source,
            short_order,
            "wire short-order manual baseline",
        )
        .with_equipment(drawbench),
    )
    .unwrap_or_else(|error| panic!("wire short baseline projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
        work_source,
        project_order,
        "wire project manual baseline",
    )
    .with_equipment(drawbench);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("wire project baseline projection failed: {error}"));
    let short_powered_projection = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
        EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
        used_condition,
        crank_condition,
        drive,
        powered_batch,
        short_order,
        "wire short-order charging",
    )
    .unwrap_or_else(|| panic!("wire short-order powered route became unavailable"));
    let project_powered_projection = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
        EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
        used_condition,
        crank_condition,
        drive,
        powered_batch,
        project_order,
        "wire project charging",
    )
    .unwrap_or_else(|| panic!("wire project powered route became unavailable"));
    let short_machine_attention =
        setup_attention + short_powered_projection.charging.attention_ticks;
    assert!(
        !clears_attention_return(
            short_baseline.duration().value(),
            short_machine_attention,
            minimum_attention_return,
        ),
        "short conductor orders should keep using the already-owned manual drawbench"
    );
    let project_machine_attention =
        setup_attention + project_powered_projection.charging.attention_ticks;
    assert!(
        clears_attention_return(
            project_baseline.duration().value(),
            project_machine_attention,
            minimum_attention_return,
        ),
        "repeated conductor work should repay the flywheel conversion by the shared capital-return floor"
    );

    let decision_state = state.clone();
    let matter_before = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("wire investment matter setup failed: {error}"))
        .total();

    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "wire project manual baseline",
    );

    let mut powered = decision_state;
    let mut executed_setup = 0_u64;
    for (request, context) in [
        (flywheel_request, "wire automation flywheel"),
        (board_request, "wire automation frame boards"),
        (handle_request, "wire automation handles"),
        (copper_request, "wire automation bearing reinforcement"),
    ] {
        executed_setup +=
            execute_manual_craft(&registries, &mut powered, request, upgrade_parts, context)
                .value();
    }
    assert_eq!(executed_setup, setup_attention);

    let powered_drawbench = validate_upgrade_equipment(
        &registries,
        &powered,
        drawbench,
        EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("manual-to-flywheel drawbench upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("flywheel drawbench upgrade commit failed: {error}"));
    assert_eq!(powered_drawbench, drawbench);
    assert_eq!(
        powered
            .equipment()
            .get_equipment(powered_drawbench)
            .map(|record| record.condition()),
        Some(used_condition),
        "flywheel conversion must preserve the used manual drawbench's condition"
    );

    let powered_project = execute_powered_project(
        &registries,
        &mut powered,
        PoweredProjectPlan {
            process: PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
            source: work_source,
            destination: powered_output,
            machine: powered_drawbench,
            crank,
            drive,
            batch: powered_batch,
            batches: project_order,
            context: "wire unattended drawing",
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
        "wire projected hand-crank wear must match execution"
    );
    assert_eq!(
        executed_setup + powered_project.charge_attention,
        project_machine_attention
    );
    assert!(project_machine_attention < baseline_ticks.value());

    let winding = CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING);
    assert_eq!(
        baseline
            .inventory()
            .get_stockpile(baseline_output)
            .map(|stockpile| stockpile.get_mass(winding)),
        Some(project_mass)
    );
    assert_eq!(
        powered
            .inventory()
            .get_stockpile(powered_output)
            .map(|stockpile| stockpile.get_mass(winding)),
        Some(project_mass),
        "powered drawing must retain the manual drawbench's lossless copper transform"
    );
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("wire baseline matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("wire investment matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("wire investment final state invalid: {error}"));
    reviewln!(
        "SETTLEMENT MACHINE EXPERIENCE family=wire-drawbench transform=electrical-winding prior=manual-drawbench upgrade=flywheel-drawbench policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t crossover:{}batches short=[batches:{} baseline:{}t machine:{}t choice:keep-prior] project=[batches:{} baseline:{}t setup:{}t charging:{}t machine:{}t attention-saved:{}t delegated:{}t power-cycle=[charges:{} max-batches-per-charge:{}] choice:upgrade] used-identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved",
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
fn flywheel_drawbench_repays_repeated_lossless_conductor_work_without_changing_yield() {
    run_wire_drawbench_investment_experience(None);
}
