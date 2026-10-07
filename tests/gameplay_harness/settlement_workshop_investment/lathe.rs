//! Spring-pole to flywheel-lathe capital crossover from raw upgrade materials.

use super::*;

const LATHE_DEMAND_SALT: u64 = 0x4C41_5448_4544_4D44;

pub(super) fn run_lathe_investment_experience(variation_root: Option<u64>) {
    let registries = build_registries();
    let batch = authored_batch(
        &registries,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        "flywheel-lathe investment",
    );
    let mut state = AppState::new();
    let bootstrap = super::super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[
            EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
            EQUIPMENT_STONE_HAND_CRANK,
        ],
        &[ENERGY_TIMBER_FRAME_FLYWHEEL_BANK],
        &[],
        "lathe prior workshop",
    );
    let (upgrade_raw, upgrade_parts) = seed_upgrade_raw(
        &registries,
        &mut state,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        "lathe flywheel upgrade",
    );
    let project_mass = Mass::from_milligrams(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES
            .checked_mul(batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("lathe disclosed input opportunity overflowed")),
    );
    let source = seed_stockpile(
        &mut state,
        project_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let _input_lot = seed_material(
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
    super::super::world_admission::admit_stationary_player(
        &registries,
        &mut state,
        &[
            bootstrap,
            upgrade_raw,
            upgrade_parts,
            source,
            baseline_output,
            powered_output,
        ],
        &[],
        "lathe investment",
    );
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
    let drive = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("lathe prior workshop-bank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("lathe prior workshop-bank commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "lathe prior workshop package must be exact"
    );

    let setup_attention = project_upgrade_setup(
        &registries,
        &state,
        upgrade_raw,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        Some(pole_lathe),
        "lathe flywheel upgrade",
    );
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("lathe hand crank disappeared before decision"));
    let pole_condition = state
        .equipment()
        .get_equipment(pole_lathe)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("lathe spring-pole machine disappeared before decision"));
    let policy = CapitalInvestmentPolicy::baseline();
    let minimum_attention_return = policy.minimum_attention_return(0, setup_attention);
    let crossover = first_attention_return_crossover(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        minimum_attention_return,
        |batches| {
            let request = select_manual_craft_request(
                &registries,
                &state,
                PROCESS_SHAPE_TIMBER_FLYWHEEL,
                source,
                batches,
                "lathe crossover spring-pole baseline",
            )
            .with_equipment(pole_lathe);
            let baseline = resolve_manual_craft(&registries, &state, &request).ok()?;
            let powered = project_powered_project(
                &registries,
                &state,
                PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
                EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
                pole_condition,
                crank_condition,
                drive,
                batch,
                batches,
                "lathe crossover charging",
            )?;
            Some((
                baseline.duration().value(),
                setup_attention
                    .checked_add(powered.charging.attention_ticks)
                    .unwrap_or_else(|| panic!("lathe crossover attention overflowed")),
            ))
        },
    )
    .unwrap_or_else(|| {
        panic!("flywheel lathe has no attention-return crossover in the disclosed opportunity")
    });
    let workloads = crossover_workloads(
        crossover,
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        variation_root.map(|root| root ^ LATHE_DEMAND_SALT),
    );
    let short_order = workloads.short_batches;
    let project_order = workloads.project_batches;
    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SHAPE_TIMBER_FLYWHEEL,
            source,
            short_order,
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
        project_order,
        "lathe project baseline",
    )
    .with_equipment(pole_lathe);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("lathe project projection failed: {error}"));
    let short_powered = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        pole_condition,
        crank_condition,
        drive,
        batch,
        short_order,
        "lathe short-order charging",
    )
    .unwrap_or_else(|| panic!("lathe short-order powered route became unavailable"));
    let project_powered = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        pole_condition,
        crank_condition,
        drive,
        batch,
        project_order,
        "lathe project charging",
    )
    .unwrap_or_else(|| panic!("lathe project powered route became unavailable"));
    let short_machine_attention = setup_attention + short_powered.charging.attention_ticks;
    let project_machine_attention = setup_attention + project_powered.charging.attention_ticks;
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
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        Some(pole_lathe),
        "lathe flywheel upgrade",
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
            destination: powered_output,
            machine: lathe,
            crank,
            drive,
            batch,
            batches: project_order,
            context: "lathe project",
        },
    );
    assert_eq!(
        powered_project.charge_attention,
        project_powered.charging.attention_ticks
    );
    assert_eq!(
        powered
            .equipment()
            .get_equipment(crank)
            .map(|record| record.condition()),
        Some(project_powered.charging.condition_after),
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
        "SETTLEMENT MACHINE EXPERIENCE family=flywheel-lathe transform=timber-flywheel prior=spring-pole-lathe policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t crossover:{}batches short=[batches:{} baseline:{}t machine:{}t choice:keep-prior] project=[batches:{} baseline:{}t setup:{}t charging:{}t machine:{}t attention-saved:{}t delegated:{}t power-cycle=[charges:{} max-batches-per-charge:{}] choice:upgrade] self-fabrication=handles-with-prior-lathe identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved",
        policy.minimum_return_ppm(),
        minimum_attention_return,
        crossover,
        short_order,
        short_baseline.duration().value(),
        short_machine_attention,
        project_order,
        project_baseline.duration().value(),
        setup_attention,
        powered_project.charge_attention,
        project_machine_attention,
        project_baseline.duration().value() - project_machine_attention,
        powered_project.delegated_ticks,
        powered_project.charge_events,
        powered_project.maximum_leg_batches,
    );
}

#[test]
fn flywheel_lathe_has_a_real_capital_crossover() {
    run_lathe_investment_experience(None);
}
