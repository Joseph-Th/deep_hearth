//! Ordinary helve-hammer investment contract.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_HELVE_HAMMER,
    EQUIPMENT_TIMBER_TREADLE_HAMMER, FORM_NATIVE_METAL, FORM_REINFORCEMENT, MATERIAL_COPPER,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
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

const HELVE_DEMAND_SALT: u64 = 0x4845_4C56_4544_4D44;

fn seed_material(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> deep_hearth::inventory::MaterialLotId {
    seed_lot(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        ROOM_TEMPERATURE,
    )
}

pub(super) fn run_helve_hammer_investment_experience(variation_root: Option<u64>) {
    let registries = build_registries();
    let helve_batch = authored_batch(
        &registries,
        PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
        "helve-hammer investment",
    );
    let mut state = AppState::new();

    let bootstrap = super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[EQUIPMENT_TIMBER_TREADLE_HAMMER, EQUIPMENT_STONE_HAND_CRANK],
        &[ENERGY_TIMBER_FRAME_FLYWHEEL_BANK],
        &[],
        "helve-hammer prior workshop",
    );
    let (upgrade_raw, upgrade_parts) = super::settlement_workshop_investment::seed_upgrade_raw(
        &registries,
        &mut state,
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        "helve-hammer investment",
    );
    let order_mass = Mass::from_milligrams(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES
            .checked_mul(helve_batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("helve disclosed input opportunity overflowed")),
    );
    let work_source = seed_stockpile(
        &mut state,
        order_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let _work_lot = seed_material(
        &registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
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
            upgrade_raw,
            upgrade_parts,
            work_source,
            baseline_output,
            powered_output,
        ],
        &[],
        "helve-hammer investment",
    );
    let treadle_hammer = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("helve investment treadle-hammer assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("helve investment treadle-hammer commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("helve investment crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("helve investment crank commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("helve investment workshop-bank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("helve investment workshop-bank commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );

    let setup_board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        upgrade_raw,
        1,
        "helve upgrade board",
    );
    let setup_board_ticks = resolve_manual_craft(&registries, &state, &setup_board_request)
        .unwrap_or_else(|error| panic!("helve upgrade board projection failed: {error}"))
        .duration()
        .value();
    let setup_handle_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        upgrade_raw,
        2,
        "helve upgrade handles",
    );
    let setup_handle_ticks = resolve_manual_craft(&registries, &state, &setup_handle_request)
        .unwrap_or_else(|error| panic!("helve upgrade handle projection failed: {error}"))
        .duration()
        .value();
    let setup_copper_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        upgrade_raw,
        1,
        "helve upgrade bearing reinforcement",
    );
    let setup_copper_ticks = resolve_manual_craft(&registries, &state, &setup_copper_request)
        .unwrap_or_else(|error| panic!("helve upgrade copper projection failed: {error}"))
        .duration()
        .value();
    let setup_attention = setup_board_ticks + setup_handle_ticks + setup_copper_ticks;
    let treadle_condition = state
        .equipment()
        .get_equipment(treadle_hammer)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("helve treadle hammer disappeared before investment decision"));
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("helve hand crank disappeared before investment decision"));
    let policy = CapitalInvestmentPolicy::baseline();
    let minimum_attention_return = policy.minimum_attention_return(0, setup_attention);
    let crossover = first_attention_return_crossover(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        minimum_attention_return,
        |batches| {
            let request = select_manual_craft_request(
                &registries,
                &state,
                PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
                work_source,
                batches,
                "helve crossover treadle baseline",
            )
            .with_equipment(treadle_hammer);
            let baseline = resolve_manual_craft(&registries, &state, &request).ok()?;
            let powered = project_powered_project(
                &registries,
                &state,
                PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
                EQUIPMENT_TIMBER_HELVE_HAMMER,
                treadle_condition,
                crank_condition,
                drive,
                helve_batch,
                batches,
                "helve crossover charging",
            )?;
            Some((
                baseline.duration().value(),
                setup_attention
                    .checked_add(powered.charging.attention_ticks)
                    .unwrap_or_else(|| panic!("helve crossover attention overflowed")),
            ))
        },
    )
    .unwrap_or_else(|| {
        panic!("helve hammer has no attention-return crossover in the disclosed opportunity")
    });
    let workloads = crossover_workloads(
        crossover,
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        variation_root.map(|root| root ^ HELVE_DEMAND_SALT),
    );
    let short_order = workloads.short_batches;
    let project_order = workloads.project_batches;
    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            work_source,
            short_order,
            "helve short-order baseline",
        )
        .with_equipment(treadle_hammer),
    )
    .unwrap_or_else(|error| panic!("helve short baseline projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        work_source,
        project_order,
        "helve project baseline",
    )
    .with_equipment(treadle_hammer);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("helve project baseline projection failed: {error}"));
    let short_powered_projection = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        treadle_condition,
        crank_condition,
        drive,
        helve_batch,
        short_order,
        "helve short-order charging",
    )
    .unwrap_or_else(|| panic!("helve short-order powered route became unavailable"));
    let project_powered_projection = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        treadle_condition,
        crank_condition,
        drive,
        helve_batch,
        project_order,
        "helve project charging",
    )
    .unwrap_or_else(|| panic!("helve project powered route became unavailable"));
    let short_machine_attention =
        setup_attention + short_powered_projection.charging.attention_ticks;
    assert!(
        !clears_attention_return(
            short_baseline.duration().value(),
            short_machine_attention,
            minimum_attention_return,
        ),
        "a short copper run must keep using the already-owned treadle hammer"
    );
    let project_machine_attention =
        setup_attention + project_powered_projection.charging.attention_ticks;
    assert!(
        clears_attention_return(
            project_baseline.duration().value(),
            project_machine_attention,
            minimum_attention_return,
        ),
        "repeated copper forming must repay the helve conversion by the shared capital-return floor"
    );

    let decision_state = state.clone();
    let matter_before = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("helve investment matter setup failed: {error}"))
        .total();
    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "helve project treadle baseline",
    );

    let mut powered = decision_state;
    let executed_setup = [
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_board_request,
            upgrade_parts,
            "helve upgrade board",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_handle_request,
            upgrade_parts,
            "helve upgrade handles",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_copper_request,
            upgrade_parts,
            "helve upgrade bearing reinforcement",
        )
        .value(),
    ]
    .into_iter()
    .try_fold(0_u64, |total, ticks| total.checked_add(ticks))
    .unwrap_or_else(|| panic!("helve setup attention overflowed"));
    assert_eq!(executed_setup, setup_attention);
    let helve = validate_upgrade_equipment(
        &registries,
        &powered,
        treadle_hammer,
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("treadle-to-helve upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("treadle-to-helve upgrade commit failed: {error}"));
    assert_eq!(helve, treadle_hammer);

    let powered_project = execute_powered_project(
        &registries,
        &mut powered,
        PoweredProjectPlan {
            process: PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
            source: work_source,
            destination: powered_output,
            machine: helve,
            crank,
            drive,
            batch: helve_batch,
            batches: project_order,
            context: "helve project unattended forging",
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
        "helve projected hand-crank wear must match execution"
    );
    assert_eq!(
        executed_setup + powered_project.charge_attention,
        project_machine_attention
    );
    assert!(project_machine_attention < baseline_ticks.value());
    assert_eq!(
        powered
            .inventory()
            .get_stockpile(powered_output)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        baseline
            .inventory()
            .get_stockpile(baseline_output)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            })
    );
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("helve baseline matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("helve matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("helve project final state invalid: {error}"));
    reviewln!(
        "SETTLEMENT MACHINE EXPERIENCE family=helve-hammer transform=copper-reinforcement prior=treadle-hammer upgrade=helve-hammer policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t crossover:{}batches short=[batches:{} baseline:{}t machine:{}t choice:keep-prior] project=[batches:{} baseline:{}t setup:{}t charging:{}t machine:{}t attention-saved:{}t delegated:{}t power-cycle=[charges:{} max-batches-per-charge:{}] choice:upgrade] identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved",
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
fn helve_hammer_converts_treadle_workshop_when_repeated_copper_work_repays_attention() {
    run_helve_hammer_investment_experience(None);
}
