//! Treadle to flywheel-grindstone capital crossover from raw upgrade materials.

use super::*;

const TOOLROOM_DEMAND_SALT: u64 = 0x544F_4F4C_444D_4E44;

pub(super) fn run_toolroom_investment_experience(variation_root: Option<u64>) {
    let registries = build_registries();
    let batch = authored_batch(
        &registries,
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        "toolroom grindstone investment",
    );
    let mut state = AppState::new();
    let bootstrap = super::super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[
            EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
            EQUIPMENT_STONE_HAND_CRANK,
        ],
        &[ENERGY_TIMBER_FRAME_FLYWHEEL_BANK],
        &[],
        "toolroom prior workshop",
    );
    let (upgrade_raw, upgrade_parts) = seed_upgrade_raw(
        &registries,
        &mut state,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        "toolroom flywheel upgrade",
    );
    let project_mass = Mass::from_milligrams(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES
            .checked_mul(batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("toolroom disclosed input opportunity overflowed")),
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
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
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
        "toolroom investment",
    );
    let treadle = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("toolroom prior treadle assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("toolroom prior treadle commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("toolroom prior hand-crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("toolroom prior hand-crank commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("toolroom prior workshop-bank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("toolroom prior workshop-bank commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "toolroom prior workshop package must be exact"
    );

    let setup_attention = project_upgrade_setup(
        &registries,
        &state,
        upgrade_raw,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        None,
        "toolroom flywheel upgrade",
    );
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("toolroom hand crank disappeared before decision"));
    let treadle_condition = state
        .equipment()
        .get_equipment(treadle)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("toolroom treadle grindstone disappeared before decision"));
    let policy = CapitalInvestmentPolicy::baseline();
    let minimum_attention_return = policy.minimum_attention_return(0, setup_attention);
    let crossover = first_attention_return_crossover(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        minimum_attention_return,
        |batches| {
            let request = select_manual_craft_request(
                &registries,
                &state,
                PROCESS_GRIND_STONE_SCRAP_TOOL,
                source,
                batches,
                "toolroom crossover treadle baseline",
            )
            .with_equipment(treadle);
            let baseline = resolve_manual_craft(&registries, &state, &request).ok()?;
            let powered = project_powered_project(
                &registries,
                &state,
                PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
                EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
                treadle_condition,
                crank_condition,
                drive,
                batch,
                batches,
                "toolroom crossover charging",
            )?;
            Some((
                baseline.duration().value(),
                setup_attention
                    .checked_add(powered.charging.attention_ticks)
                    .unwrap_or_else(|| panic!("toolroom crossover attention overflowed")),
            ))
        },
    )
    .unwrap_or_else(|| {
        panic!("flywheel grindstone has no attention-return crossover in the disclosed opportunity")
    });
    let workloads = crossover_workloads(
        crossover,
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        variation_root.map(|root| root ^ TOOLROOM_DEMAND_SALT),
    );
    let short_order = workloads.short_batches;
    let project_order = workloads.project_batches;
    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_GRIND_STONE_SCRAP_TOOL,
            source,
            short_order,
            "toolroom short-order baseline",
        )
        .with_equipment(treadle),
    )
    .unwrap_or_else(|error| panic!("toolroom short-order projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_GRIND_STONE_SCRAP_TOOL,
        source,
        project_order,
        "toolroom project baseline",
    )
    .with_equipment(treadle);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("toolroom project projection failed: {error}"));
    let short_powered = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        treadle_condition,
        crank_condition,
        drive,
        batch,
        short_order,
        "toolroom short-order charging",
    )
    .unwrap_or_else(|| panic!("toolroom short-order powered route became unavailable"));
    let project_powered = project_powered_project(
        &registries,
        &state,
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        treadle_condition,
        crank_condition,
        drive,
        batch,
        project_order,
        "toolroom project charging",
    )
    .unwrap_or_else(|| panic!("toolroom project powered route became unavailable"));
    let short_machine_attention = setup_attention + short_powered.charging.attention_ticks;
    let project_machine_attention = setup_attention + project_powered.charging.attention_ticks;
    assert!(
        !clears_attention_return(
            short_baseline.duration().value(),
            short_machine_attention,
            minimum_attention_return,
        ),
        "short service-stock work must retain the paid-off treadle grindstone"
    );
    assert!(
        clears_attention_return(
            project_baseline.duration().value(),
            project_machine_attention,
            minimum_attention_return,
        ),
        "repeated service-stock work must repay flywheel-grindstone conversion"
    );

    let decision_state = state.clone();
    let matter_before = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("toolroom investment matter setup failed: {error}"))
        .total();
    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "toolroom project treadle baseline",
    );
    assert_eq!(baseline_ticks, project_baseline.duration());

    let mut powered = decision_state;
    let executed_setup = execute_upgrade_setup(
        &registries,
        &mut powered,
        upgrade_raw,
        upgrade_parts,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        None,
        "toolroom flywheel upgrade",
    );
    assert_eq!(executed_setup, setup_attention);
    let grindstone = validate_upgrade_equipment(
        &registries,
        &powered,
        treadle,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("flywheel-grindstone upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("flywheel-grindstone upgrade commit failed: {error}"));
    assert_eq!(
        grindstone, treadle,
        "toolroom conversion must preserve equipment identity"
    );
    let powered_project = execute_powered_project(
        &registries,
        &mut powered,
        PoweredProjectPlan {
            process: PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
            source,
            destination: powered_output,
            machine: grindstone,
            crank,
            drive,
            batch,
            batches: project_order,
            context: "toolroom project",
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
        "toolroom projected crank wear must match execution"
    );
    assert_eq!(
        executed_setup + powered_project.charge_attention,
        project_machine_attention,
        "toolroom decision projection must match executed player attention"
    );
    assert_output_equivalence(
        &baseline,
        baseline_output,
        &powered,
        powered_output,
        &[
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        ],
        "toolroom project",
    );
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("toolroom baseline matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("toolroom powered matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("toolroom investment final state invalid: {error}"));
    reviewln!(
        "SETTLEMENT MACHINE EXPERIENCE family=flywheel-grindstone transform=service-stock prior=treadle-grindstone policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t crossover:{}batches short=[batches:{} baseline:{}t machine:{}t choice:keep-prior] project=[batches:{} baseline:{}t setup:{}t charging:{}t machine:{}t attention-saved:{}t delegated:{}t power-cycle=[charges:{} max-batches-per-charge:{}] choice:upgrade] identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved",
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
fn flywheel_toolroom_has_a_real_capital_crossover() {
    run_toolroom_investment_experience(None);
}
