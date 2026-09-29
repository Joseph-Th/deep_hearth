//! Spring-pole to flywheel-lathe capital crossover from raw upgrade materials.

use super::*;

pub(super) fn run_lathe_investment_experience() {
    let registries = build_registries();
    let batch = authored_batch(
        &registries,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        "flywheel-lathe investment",
    );
    let mut state = AppState::new();
    let bootstrap = seed_stockpile(
        &mut state,
        Mass::from_milligrams(5_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(1_600_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(1_800_000),
        ),
    ] {
        seed_material(&registries, &mut state, bootstrap, commodity, mass);
    }
    let (upgrade_raw, upgrade_parts) = seed_common_upgrade_raw(&registries, &mut state);
    let project_mass = Mass::from_milligrams(
        PROJECT_LATHE_ORDER
            .checked_mul(batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("lathe project input mass overflowed")),
    );
    let source = seed_stockpile(
        &mut state,
        project_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let input_lot = seed_material(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        project_mass,
    );
    let baseline_output = seed_stockpile(
        &mut state,
        project_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let powered_output = seed_stockpile(
        &mut state,
        project_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("lathe investment survival setup failed: {error}"));
    let pole_lathe = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("lathe prior spring-pole assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("lathe prior spring-pole commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("lathe prior hand-crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("lathe prior hand-crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("lathe prior flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("lathe prior flywheel commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "lathe prior workshop package must be exact"
    );

    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SHAPE_TIMBER_FLYWHEEL,
            source,
            SHORT_LATHE_ORDER,
            "lathe short-order baseline",
        )
        .with_equipment(pole_lathe),
    )
    .unwrap_or_else(|error| panic!("lathe short-order projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_TIMBER_FLYWHEEL,
        source,
        PROJECT_LATHE_ORDER,
        "lathe project baseline",
    )
    .with_equipment(pole_lathe);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("lathe project projection failed: {error}"));
    let setup_attention = project_upgrade_setup(&registries, &state, upgrade_raw, Some(pole_lathe));
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("lathe hand crank disappeared before decision"));
    let short_charge = project_manual_power_sequence(
        &registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: batch.work,
            charges: SHORT_LATHE_ORDER,
        },
        "lathe short-order charging",
    );
    let project_charge = project_manual_power_sequence(
        &registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: batch.work,
            charges: PROJECT_LATHE_ORDER,
        },
        "lathe project charging",
    );
    let policy = CapitalInvestmentPolicy::baseline();
    let minimum_attention_return = policy.minimum_attention_return(0, setup_attention);
    let short_machine_attention = setup_attention + short_charge.attention_ticks;
    let project_machine_attention = setup_attention + project_charge.attention_ticks;
    assert!(
        !clears_attention_return(
            short_baseline.duration().value(),
            short_machine_attention,
            minimum_attention_return,
        ),
        "short timber turning must retain the paid-off spring-pole lathe"
    );
    assert!(
        clears_attention_return(
            project_baseline.duration().value(),
            project_machine_attention,
            minimum_attention_return,
        ),
        "repeated timber turning must repay flywheel-lathe conversion"
    );

    let decision_state = state.clone();
    let matter_before = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("lathe investment matter setup failed: {error}"))
        .total();
    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "lathe project spring-pole baseline",
    );
    assert_eq!(baseline_ticks, project_baseline.duration());

    let mut powered = decision_state;
    let executed_setup = execute_upgrade_setup(
        &registries,
        &mut powered,
        upgrade_raw,
        upgrade_parts,
        Some(pole_lathe),
    );
    assert_eq!(executed_setup, setup_attention);
    let lathe = validate_upgrade_equipment(
        &registries,
        &powered,
        pole_lathe,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("flywheel-lathe upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("flywheel-lathe upgrade commit failed: {error}"));
    assert_eq!(lathe, pole_lathe, "lathe conversion must preserve identity");
    let powered_project = execute_powered_project(
        &registries,
        &mut powered,
        PoweredProjectPlan {
            process: PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
            source,
            input_lot,
            destination: powered_output,
            machine: lathe,
            crank,
            drive,
            batch,
            batches: PROJECT_LATHE_ORDER,
            context: "lathe project",
        },
    );
    assert_eq!(
        powered_project.charge_attention,
        project_charge.attention_ticks
    );
    assert_eq!(
        powered
            .equipment()
            .get_equipment(crank)
            .map(|record| record.condition()),
        Some(project_charge.condition_after),
        "lathe projected crank wear must match execution"
    );
    assert_eq!(
        executed_setup + powered_project.charge_attention,
        project_machine_attention,
        "lathe decision projection must match executed player attention"
    );
    assert_output_equivalence(
        &baseline,
        baseline_output,
        &powered,
        powered_output,
        &[
            CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        ],
        "lathe project",
    );
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("lathe baseline matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("lathe powered matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("lathe investment final state invalid: {error}"));
    reviewln!(
        "SETTLEMENT MACHINE EXPERIENCE family=flywheel-lathe transform=timber-flywheel prior=spring-pole-lathe policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t short=[batches:{} baseline:{}t machine:{}t choice:keep-prior] project=[batches:{} baseline:{}t setup:{}t charging:{}t machine:{}t attention-saved:{}t delegated:{}t choice:upgrade] self-fabrication=handles-with-prior-lathe identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved",
        policy.minimum_return_ppm(),
        minimum_attention_return,
        SHORT_LATHE_ORDER,
        short_baseline.duration().value(),
        short_machine_attention,
        PROJECT_LATHE_ORDER,
        project_baseline.duration().value(),
        setup_attention,
        powered_project.charge_attention,
        project_machine_attention,
        project_baseline.duration().value() - project_machine_attention,
        powered_project.delegated_ticks,
    );
}

#[test]
fn flywheel_lathe_has_a_real_capital_crossover() {
    run_lathe_investment_experience();
}
