//! Ordinary settlement-machine investment contracts.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_SPRING_POLE_LATHE, EQUIPMENT_TIMBER_TREADLE_GRINDSTONE, FORM_BOARD, FORM_CHIP,
    FORM_FLYWHEEL, FORM_LOG, FORM_SCRAP, FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_STONE,
    MATERIAL_WOOD, PROCESS_GRIND_STONE_SCRAP_TOOL, PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
    PROCESS_POWER_TURN_TIMBER_FLYWHEEL, PROCESS_SHAPE_TIMBER_FLYWHEEL, build_registries,
};
#[cfg(test)]
use deep_hearth::content::{
    EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_TIMBER_SASH_SAWMILL,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_POWER_SAW_WOOD_BOARDS, PROCESS_SAW_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    PoweredCraftRequest, project_powered_craft_work, resolve_manual_craft,
    validate_start_powered_craft,
};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::{MaterialLotSelection, StockpileStorageProfile};
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;

#[cfg(test)]
use super::capital_investment_policy::{
    CapitalInvestmentPolicy, clears_attention_return, first_attention_return_crossover,
};
use super::environment::ROOM_TEMPERATURE;
#[cfg(test)]
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::powered_craft_planning::authored_batch;
use super::production_timing::finish_uninterrupted_production_job;
#[cfg(test)]
use super::settlement_generation::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, crossover_workloads,
};
#[cfg(test)]
use super::settlement_power_planning::{ManualPowerSequenceRequest, project_manual_power_sequence};

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

fn authored_powered_output_mass(
    registries: &deep_hearth::registry::Registries,
    process: deep_hearth::production::ProcessId,
    commodity: CommodityKey,
    context: &'static str,
) -> Mass {
    let powered = registries
        .crafting()
        .get_powered(process)
        .unwrap_or_else(|| panic!("{context} powered craft disappeared"));
    let transform = registries
        .crafting()
        .get_manual(powered.transform())
        .unwrap_or_else(|| panic!("{context} manual transform disappeared"));
    transform
        .outputs()
        .iter()
        .filter(|output| output.commodity() == commodity)
        .map(|output| output.mass())
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(mass))
        .filter(|mass| !mass.is_zero())
        .unwrap_or_else(|| panic!("{context} authored output commodity disappeared"))
}

#[test]
fn sash_sawmill_upgrades_existing_workshop_only_when_disclosed_lumber_demand_repays_attention() {
    let registries = build_registries();
    let sawmill_batch = authored_batch(
        &registries,
        PROCESS_POWER_SAW_WOOD_BOARDS,
        "sawmill investment",
    );
    let mut state = AppState::new();

    // Disclosed bootstrap: the settlement already owns the earlier frame-saw and mechanical-work
    // tier. The decision under test is whether to keep using that durable infrastructure or spend
    // current raw material and attention converting it into unattended sawing capacity.
    let bootstrap = super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_STONE_HAND_CRANK],
        &[ENERGY_STONE_FLYWHEEL_DRIVE],
        &[],
        "sawmill prior workshop",
    );
    let (upgrade_raw, upgrade_parts) = super::settlement_workshop_investment::seed_upgrade_raw(
        &registries,
        &mut state,
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        "sawmill investment",
    );
    let project_input_mass = Mass::from_milligrams(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES
            .checked_mul(sawmill_batch.input_mass.milligrams())
            .unwrap_or_else(|| panic!("sawmill disclosed input opportunity overflowed")),
    );
    let work_source = seed_stockpile(
        &mut state,
        project_input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let work_lot = seed_material(
        &registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        project_input_mass,
    );
    let baseline_output = seed_stockpile(
        &mut state,
        project_input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let powered_output = seed_stockpile(
        &mut state,
        project_input_mass,
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
        "sawmill investment",
    );
    let frame_saw = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("sawmill investment frame-saw assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("sawmill investment frame-saw commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("sawmill investment crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("sawmill investment crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("sawmill investment flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("sawmill investment flywheel commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "disclosed prior infrastructure package must consume exactly its authored components"
    );

    let setup_board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SAW_WOOD_BOARDS,
        upgrade_raw,
        3,
        "sawmill upgrade boards",
    )
    .with_equipment(frame_saw);
    let setup_board_ticks = resolve_manual_craft(&registries, &state, &setup_board_request)
        .unwrap_or_else(|error| panic!("sawmill upgrade board projection failed: {error}"))
        .duration()
        .value();
    let setup_handle_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        upgrade_raw,
        3,
        "sawmill upgrade handles",
    );
    let setup_handle_ticks = resolve_manual_craft(&registries, &state, &setup_handle_request)
        .unwrap_or_else(|error| panic!("sawmill upgrade handle projection failed: {error}"))
        .duration()
        .value();
    let setup_copper_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        upgrade_raw,
        1,
        "sawmill upgrade reinforcement",
    );
    let setup_copper_ticks = resolve_manual_craft(&registries, &state, &setup_copper_request)
        .unwrap_or_else(|error| panic!("sawmill upgrade copper projection failed: {error}"))
        .duration()
        .value();
    let setup_attention = setup_board_ticks + setup_handle_ticks + setup_copper_ticks;
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("sawmill hand crank disappeared before investment decision"));
    let minimum_attention_return =
        CapitalInvestmentPolicy::baseline().minimum_attention_return(0, setup_attention);
    assert!(minimum_attention_return > 0);
    let crossover = first_attention_return_crossover(
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        minimum_attention_return,
        |batches| {
            let request = select_manual_craft_request(
                &registries,
                &state,
                PROCESS_SAW_WOOD_BOARDS,
                work_source,
                batches,
                "sawmill crossover frame-saw baseline",
            )
            .with_equipment(frame_saw);
            let baseline = resolve_manual_craft(&registries, &state, &request).ok()?;
            let charge = project_manual_power_sequence(
                &registries,
                ManualPowerSequenceRequest {
                    method: MANUAL_POWER_HAND_CRANK,
                    equipment: EQUIPMENT_STONE_HAND_CRANK,
                    starting_condition: crank_condition,
                    store: ENERGY_STONE_FLYWHEEL_DRIVE,
                    energy_per_charge: sawmill_batch.work,
                    charges: batches,
                },
                "sawmill crossover charging",
            );
            Some((
                baseline.duration().value(),
                setup_attention
                    .checked_add(charge.attention_ticks)
                    .unwrap_or_else(|| panic!("sawmill crossover attention overflowed")),
            ))
        },
    )
    .unwrap_or_else(|| {
        panic!("sash sawmill has no attention-return crossover in the disclosed opportunity")
    });
    let workloads = crossover_workloads(
        crossover,
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        None,
    );
    let short_order = workloads.short_batches;
    let marginal_order = workloads.marginal_batches;
    let project_order = workloads.project_batches;
    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SAW_WOOD_BOARDS,
            work_source,
            short_order,
            "sawmill short-order baseline",
        )
        .with_equipment(frame_saw),
    )
    .unwrap_or_else(|error| panic!("sawmill short baseline projection failed: {error}"));
    let marginal_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SAW_WOOD_BOARDS,
            work_source,
            marginal_order,
            "sawmill marginal-order baseline",
        )
        .with_equipment(frame_saw),
    )
    .unwrap_or_else(|error| panic!("sawmill marginal baseline projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SAW_WOOD_BOARDS,
        work_source,
        project_order,
        "sawmill project baseline",
    )
    .with_equipment(frame_saw);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("sawmill project baseline projection failed: {error}"));
    let short_charge_projection = project_manual_power_sequence(
        &registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: sawmill_batch.work,
            charges: short_order,
        },
        "sawmill short-order charging",
    );
    let marginal_charge_projection = project_manual_power_sequence(
        &registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: sawmill_batch.work,
            charges: marginal_order,
        },
        "sawmill marginal-order charging",
    );
    let project_charge_projection = project_manual_power_sequence(
        &registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: sawmill_batch.work,
            charges: project_order,
        },
        "sawmill project charging",
    );
    let short_machine_attention = setup_attention + short_charge_projection.attention_ticks;
    let marginal_machine_attention = setup_attention + marginal_charge_projection.attention_ticks;
    let project_machine_attention = setup_attention + project_charge_projection.attention_ticks;
    assert!(
        !clears_attention_return(
            short_baseline.duration().value(),
            short_machine_attention,
            minimum_attention_return,
        ),
        "small lumber orders must keep using the already-owned frame saw instead of forcing mechanization"
    );
    assert!(
        !clears_attention_return(
            marginal_baseline.duration().value(),
            marginal_machine_attention,
            minimum_attention_return,
        ),
        "a token attention win must not consume settlement capital for sawmill conversion"
    );
    assert!(
        clears_attention_return(
            project_baseline.duration().value(),
            project_machine_attention,
            minimum_attention_return,
        ),
        "a disclosed settlement lumber project must be large enough to repay sawmill conversion and charging attention"
    );

    let decision_state = state.clone();
    let initial_matter = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("sawmill investment matter setup failed: {error}"))
        .total();

    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "sawmill project frame-saw baseline",
    );
    assert_eq!(baseline_ticks, project_baseline.duration());

    let mut powered = decision_state;
    let executed_setup = [
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_board_request,
            upgrade_parts,
            "sawmill upgrade boards",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_handle_request,
            upgrade_parts,
            "sawmill upgrade handles",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_copper_request,
            upgrade_parts,
            "sawmill upgrade reinforcement",
        )
        .value(),
    ]
    .into_iter()
    .try_fold(0_u64, |total, ticks| total.checked_add(ticks))
    .unwrap_or_else(|| panic!("sawmill setup attention overflowed"));
    assert_eq!(executed_setup, setup_attention);
    let sawmill = validate_upgrade_equipment(
        &registries,
        &powered,
        frame_saw,
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("frame-saw to sash-sawmill upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("frame-saw to sash-sawmill upgrade commit failed: {error}"));
    assert_eq!(
        sawmill, frame_saw,
        "mechanization must preserve equipment identity"
    );

    let mut executed_charge_attention = 0_u64;
    let mut powered_elapsed = 0_u64;
    for _ in 0..project_order {
        let work = validate_start_manual_power(
            &registries,
            &powered,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, sawmill_batch.work),
        )
        .unwrap_or_else(|error| panic!("sawmill project charging failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("sawmill project charging commit failed: {error}"));
        let charged =
            finish_manual_power_work(&registries, &mut powered, work, "sawmill project charge");
        executed_charge_attention += charged;

        let job = validate_start_powered_craft(
            &registries,
            &powered,
            PoweredCraftRequest::single(
                PROCESS_POWER_SAW_WOOD_BOARDS,
                work_source,
                MaterialLotSelection::new(work_lot, sawmill_batch.input_mass),
                sawmill,
                drive,
            ),
            powered_output,
        )
        .unwrap_or_else(|error| panic!("sawmill project start failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("sawmill project commit failed: {error}"));
        let duration = powered
            .production()
            .get_job(job)
            .map(|record| record.active_duration().value())
            .unwrap_or_else(|| panic!("sawmill project job disappeared after admission"));
        assert_eq!(powered.player_work().active(), None);
        finish_uninterrupted_production_job(
            &registries,
            &mut powered,
            job,
            "sawmill project unattended sawing",
        );
        powered_elapsed += duration;
    }
    assert_eq!(
        executed_charge_attention,
        project_charge_projection.attention_ticks
    );
    assert_eq!(
        powered
            .equipment()
            .get_equipment(crank)
            .map(|record| record.condition()),
        Some(project_charge_projection.condition_after),
        "sawmill projected hand-crank wear must match execution"
    );
    assert_eq!(
        executed_setup + executed_charge_attention,
        project_machine_attention,
        "pre-action attention estimate must match the executed mechanization package"
    );
    assert!(project_machine_attention < baseline_ticks.value());
    assert!(
        powered_elapsed > 0,
        "delegated machine work must still occupy world time"
    );

    let baseline_stockpile = baseline
        .inventory()
        .get_stockpile(baseline_output)
        .unwrap_or_else(|| panic!("frame-saw baseline output disappeared"));
    let powered_stockpile = powered
        .inventory()
        .get_stockpile(powered_output)
        .unwrap_or_else(|| panic!("sawmill output disappeared"));
    for commodity in [
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
    ] {
        assert_eq!(
            powered_stockpile.get_mass(commodity),
            baseline_stockpile.get_mass(commodity),
            "mechanization must preserve the learned frame-saw material transform"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("frame-saw baseline matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("sawmill matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("sawmill project final state invalid: {error}"));
}

pub(super) fn run_timbershop_lathe_delegation_experience() {
    let registries = build_registries();
    let lathe_batch = authored_batch(
        &registries,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        "flywheel-lathe settlement",
    );
    let mut state = AppState::new();

    let bootstrap = super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[
            EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
            EQUIPMENT_STONE_HAND_CRANK,
        ],
        &[ENERGY_STONE_FLYWHEEL_DRIVE],
        &[EQUIPMENT_TIMBER_FLYWHEEL_LATHE],
        "lathe settlement package",
    );

    let source = seed_stockpile(
        &mut state,
        lathe_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let log = seed_material(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        lathe_batch.input_mass,
    );
    let output = seed_stockpile(
        &mut state,
        lathe_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );

    super::world_admission::admit_stationary_player(
        &registries,
        &mut state,
        &[bootstrap, source, output],
        &[],
        "lathe settlement",
    );
    let pole_lathe = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("spring-pole lathe settlement assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("spring-pole lathe settlement commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("lathe settlement crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("lathe settlement crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("lathe settlement flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("lathe settlement flywheel commit failed: {error}"));

    let hand = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SHAPE_TIMBER_FLYWHEEL,
            source,
            1,
            "timber flywheel hand-turning baseline",
        ),
    )
    .unwrap_or_else(|error| panic!("hand flywheel turning projection failed: {error}"));
    let pole = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SHAPE_TIMBER_FLYWHEEL,
            source,
            1,
            "spring-pole flywheel turning",
        )
        .with_equipment(pole_lathe),
    )
    .unwrap_or_else(|error| panic!("spring-pole flywheel projection failed: {error}"));
    assert!(hand.duration().value() > 0);
    assert!(pole.duration() < hand.duration());
    assert_eq!(pole.output_streams(), hand.output_streams());

    let lathe = validate_upgrade_equipment(
        &registries,
        &state,
        pole_lathe,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("flywheel-lathe settlement upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel-lathe settlement upgrade commit failed: {error}"));
    assert_eq!(lathe, pole_lathe);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "lathe settlement package must consume exactly the authored upgrade, crank, and flywheel components"
    );

    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("lathe settlement matter setup failed: {error}"))
        .total();

    let work = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, lathe_batch.work),
    )
    .unwrap_or_else(|error| panic!("lathe settlement charge failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("lathe settlement charge commit failed: {error}"));
    let charge_attention =
        finish_manual_power_work(&registries, &mut state, work, "lathe settlement charge");
    assert!(charge_attention > 0);
    assert!(
        charge_attention < pole.duration().value(),
        "stored-work turning must reduce player attention versus the spring-pole lathe"
    );

    let powered_projection = project_powered_craft_work(
        &registries,
        &state,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        lathe_batch.input_mass,
        lathe,
        drive,
    )
    .unwrap_or_else(|error| panic!("lathe settlement powered projection failed: {error}"));
    let job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
            source,
            MaterialLotSelection::new(log, lathe_batch.input_mass),
            lathe,
            drive,
        ),
        output,
    )
    .unwrap_or_else(|error| panic!("lathe settlement powered turning failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("lathe settlement powered turning commit failed: {error}"));
    assert_eq!(
        state.player_work().active(),
        None,
        "flywheel lathe must release the player after the finite work charge is stored"
    );
    let powered_ticks = state
        .production()
        .get_job(job)
        .map(|record| record.active_duration().value())
        .unwrap_or_else(|| panic!("lathe settlement powered job disappeared"));
    assert_eq!(
        powered_ticks,
        powered_projection.duration().value(),
        "committed flywheel-lathe duration must match the canonical powered-work projection"
    );
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        job,
        "lathe settlement unattended turning",
    );

    let output_stockpile = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("lathe settlement output disappeared"));
    for commodity in [
        CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
    ] {
        assert_eq!(
            output_stockpile.get_mass(commodity),
            authored_powered_output_mass(
                &registries,
                PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
                commodity,
                "lathe settlement",
            ),
            "executed flywheel-lathe output must match the authored transform"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("lathe settlement matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("lathe settlement final state invalid: {error}"));
    reviewln!(
        "SETTLEMENT DELEGATION EXPERIENCE family=flywheel-lathe transform=timber-flywheel prior=spring-pole-lathe direct=[hand:{}t assisted:{}t] stored-work=[charge:{}t autonomous:{}t active-saving:{}t] identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved scope=delegation-not-capital-crossover",
        hand.duration().value(),
        pole.duration().value(),
        charge_attention,
        powered_ticks,
        pole.duration().value() - charge_attention,
    );
}

#[test]
fn timber_lathe_upgrade_converts_direct_turning_into_finite_work_delegation() {
    run_timbershop_lathe_delegation_experience();
}

pub(super) fn run_toolroom_grindstone_delegation_experience() {
    let registries = build_registries();
    let toolroom_batch = authored_batch(
        &registries,
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        "toolroom settlement",
    );
    let mut state = AppState::new();

    let bootstrap = super::settlement_fixture::seed_inherited_workshop_package(
        &registries,
        &mut state,
        &[
            EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
            EQUIPMENT_STONE_HAND_CRANK,
        ],
        &[ENERGY_STONE_FLYWHEEL_DRIVE],
        &[EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH],
        "toolroom settlement package",
    );
    let source = seed_stockpile(
        &mut state,
        toolroom_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let scrap = seed_material(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        toolroom_batch.input_mass,
    );
    let output = seed_stockpile(
        &mut state,
        toolroom_batch.input_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    super::world_admission::admit_stationary_player(
        &registries,
        &mut state,
        &[bootstrap, source, output],
        &[],
        "toolroom settlement",
    );

    let treadle = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("treadle grindstone settlement assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("treadle grindstone settlement commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("toolroom settlement crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("toolroom settlement crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("toolroom settlement flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("toolroom settlement flywheel commit failed: {error}"));

    let treadle_projection = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_GRIND_STONE_SCRAP_TOOL,
            source,
            1,
            "toolroom treadle recovery baseline",
        )
        .with_equipment(treadle),
    )
    .unwrap_or_else(|error| panic!("treadle service-stock projection failed: {error}"));
    assert!(treadle_projection.duration().value() > 0);

    let powered = validate_upgrade_equipment(
        &registries,
        &state,
        treadle,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("toolroom flywheel upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("toolroom flywheel upgrade commit failed: {error}"));
    assert_eq!(
        powered, treadle,
        "toolroom upgrade must preserve equipment identity"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "toolroom package must consume exactly its disclosed components"
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("toolroom settlement matter setup failed: {error}"))
        .total();

    let work = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, toolroom_batch.work),
    )
    .unwrap_or_else(|error| panic!("toolroom settlement charge failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("toolroom settlement charge commit failed: {error}"));
    let charge_attention =
        finish_manual_power_work(&registries, &mut state, work, "toolroom settlement charge");
    assert!(charge_attention < treadle_projection.duration().value());

    let powered_projection = project_powered_craft_work(
        &registries,
        &state,
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        toolroom_batch.input_mass,
        powered,
        drive,
    )
    .unwrap_or_else(|error| panic!("toolroom settlement powered projection failed: {error}"));
    let job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
            source,
            MaterialLotSelection::new(scrap, toolroom_batch.input_mass),
            powered,
            drive,
        ),
        output,
    )
    .unwrap_or_else(|error| panic!("toolroom settlement powered recovery failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("toolroom settlement powered recovery commit failed: {error}"));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(|record| record.active_duration()),
        Some(powered_projection.duration()),
        "committed toolroom duration must match the canonical powered-work projection"
    );
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        job,
        "toolroom settlement unattended recovery",
    );

    let output_stockpile = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("toolroom settlement output disappeared"));
    for commodity in [
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
    ] {
        assert_eq!(
            output_stockpile.get_mass(commodity),
            authored_powered_output_mass(
                &registries,
                PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
                commodity,
                "toolroom settlement",
            ),
            "executed toolroom output must match the authored transform"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("toolroom settlement matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("toolroom settlement final state invalid: {error}"));
    reviewln!(
        "SETTLEMENT DELEGATION EXPERIENCE family=flywheel-grindstone transform=service-stock prior=treadle-grindstone direct=[assisted:{}t] stored-work=[charge:{}t autonomous:{}t active-saving:{}t] identity-preserved=true exact-yield-preserved=true finite-stored-work=true matter=conserved scope=delegation-not-capital-crossover",
        treadle_projection.duration().value(),
        charge_attention,
        powered_projection.duration().value(),
        treadle_projection.duration().value() - charge_attention,
    );
}

#[test]
fn toolroom_grindstone_upgrade_turns_worn_stone_into_delegated_service_stock() {
    run_toolroom_grindstone_delegation_experience();
}
