//! Ordinary manual-to-powered wire-drawing investment contract.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_WIRE_DRAWBENCH, FORM_ELECTRICAL_WINDING, FORM_LOG, FORM_LUMP,
    FORM_NATIVE_METAL, FORM_REINFORCEMENT, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_DRAW_COPPER_ELECTRICAL_WINDING, PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
    PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
    build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    PoweredCraftRequest, resolve_manual_craft, validate_start_powered_craft,
};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;

use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::powered_craft_planning::authored_batch;
use super::production_timing::finish_uninterrupted_production_job;
use super::settlement_power_planning::{ManualPowerSequenceRequest, project_manual_power_sequence};

const SHORT_WINDING_ORDER: u64 = 8;
const PROJECT_WINDING_ORDER: u64 = 14;

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

pub(super) fn run_wire_drawbench_investment_experience() {
    let registries = build_registries();
    let powered_batch = authored_batch(
        &registries,
        PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
        "flywheel drawbench investment",
    );

    let mut state = AppState::new();

    // The decision begins with the manual lossless drawbench already owned. The bootstrap stock
    // also contains a crank and one finite flywheel store so the comparison prices only the
    // additional automation conversion rather than unrelated first-workshop acquisition.
    let bootstrap = super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[EQUIPMENT_TIMBER_WIRE_DRAWBENCH, EQUIPMENT_STONE_HAND_CRANK],
        &[ENERGY_STONE_FLYWHEEL_DRIVE],
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
    // Every automation addition is fabricated from ordinary raw stone, logs, and native copper.
    let upgrade_raw = seed_stockpile(
        &mut state,
        Mass::from_milligrams(5_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(4_000_000),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
    );
    let upgrade_parts = seed_stockpile(
        &mut state,
        Mass::from_milligrams(5_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );

    let order_mass = Mass::from_milligrams(
        PROJECT_WINDING_ORDER
            .checked_mul(powered_batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("wire project input mass overflowed")),
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
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("wire investment flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("wire investment flywheel commit failed: {error}"));
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

    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
            work_source,
            SHORT_WINDING_ORDER,
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
        PROJECT_WINDING_ORDER,
        "wire project manual baseline",
    )
    .with_equipment(drawbench);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("wire project baseline projection failed: {error}"));

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
    let short_charge_projection = project_manual_power_sequence(
        &registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: powered_batch.work,
            charges: SHORT_WINDING_ORDER,
        },
        "wire short-order charging",
    );
    let project_charge_projection = project_manual_power_sequence(
        &registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: powered_batch.work,
            charges: PROJECT_WINDING_ORDER,
        },
        "wire project charging",
    );
    let policy = CapitalInvestmentPolicy::baseline();
    let minimum_attention_return = policy.minimum_attention_return(0, setup_attention);
    let short_machine_attention = setup_attention + short_charge_projection.attention_ticks;
    assert!(
        !clears_attention_return(
            short_baseline.duration().value(),
            short_machine_attention,
            minimum_attention_return,
        ),
        "short conductor orders should keep using the already-owned manual drawbench"
    );
    let project_machine_attention = setup_attention + project_charge_projection.attention_ticks;
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

    let mut charge_attention = 0_u64;
    let mut delegated_ticks = 0_u64;
    for _ in 0..PROJECT_WINDING_ORDER {
        let charge = validate_start_manual_power(
            &registries,
            &powered,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, powered_batch.work),
        )
        .unwrap_or_else(|error| panic!("wire project charge failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("wire project charge commit failed: {error}"));
        charge_attention +=
            finish_manual_power_work(&registries, &mut powered, charge, "wire project charge");

        let selections = select_stockpile_mass(
            &powered,
            work_source,
            powered_batch.input_mass,
            "wire powered input",
        );
        let job = validate_start_powered_craft(
            &registries,
            &powered,
            PoweredCraftRequest::new(
                PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING,
                work_source,
                selections,
                powered_drawbench,
                drive,
            ),
            powered_output,
        )
        .unwrap_or_else(|error| panic!("wire powered start failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("wire powered commit failed: {error}"));
        assert_eq!(powered.player_work().active(), None);
        delegated_ticks = delegated_ticks
            .checked_add(
                powered
                    .production()
                    .get_job(job)
                    .map(|record| record.active_duration().value())
                    .unwrap_or_else(|| panic!("wire powered job disappeared before completion")),
            )
            .unwrap_or_else(|| panic!("wire delegated duration overflowed"));
        finish_uninterrupted_production_job(
            &registries,
            &mut powered,
            job,
            "wire unattended drawing",
        );
    }

    assert_eq!(charge_attention, project_charge_projection.attention_ticks);
    assert_eq!(
        powered
            .equipment()
            .get_equipment(crank)
            .map(|record| record.condition()),
        Some(project_charge_projection.condition_after),
        "wire projected hand-crank wear must match execution"
    );
    assert_eq!(executed_setup + charge_attention, project_machine_attention);
    assert!(project_machine_attention < baseline_ticks.value());

    let winding = CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING);
    assert_eq!(
        baseline
            .inventory()
            .get_stockpile(baseline_output)
            .map(|stockpile| stockpile.get_mass(winding)),
        Some(order_mass)
    );
    assert_eq!(
        powered
            .inventory()
            .get_stockpile(powered_output)
            .map(|stockpile| stockpile.get_mass(winding)),
        Some(order_mass),
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
        "SETTLEMENT MACHINE EXPERIENCE family=wire-drawbench transform=electrical-winding prior=manual-drawbench upgrade=flywheel-drawbench policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t short=[batches:{} baseline:{}t machine:{}t choice:keep-prior] project=[batches:{} baseline:{}t setup:{}t charging:{}t machine:{}t attention-saved:{}t delegated:{}t choice:upgrade] used-identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved",
        policy.minimum_return_ppm(),
        minimum_attention_return,
        SHORT_WINDING_ORDER,
        short_baseline.duration().value(),
        short_machine_attention,
        PROJECT_WINDING_ORDER,
        project_baseline.duration().value(),
        setup_attention,
        project_charge_projection.attention_ticks,
        project_machine_attention,
        project_baseline.duration().value() - project_machine_attention,
        delegated_ticks,
    );
}

#[test]
fn flywheel_drawbench_repays_repeated_lossless_conductor_work_without_changing_yield() {
    run_wire_drawbench_investment_experience();
}
