//! Ordinary settlement lumber investment episode over disclosed prior workshop infrastructure.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_BOARD, FORM_CHIP, FORM_LOG, FORM_NATIVE_METAL,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_WOOD, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    PoweredCraftRequest, project_manual_craft_equipment, resolve_manual_craft,
    validate_start_powered_craft,
};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{
    EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};

use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::{plan_manual_craft_request, select_manual_craft_request};
use super::manual_craft_topology_planning::manual_craft_topology_plan_for_output_from_inputs;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::physical_time::format_physical_duration;
use super::powered_craft_planning::authored_batch;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;
use super::settlement_power_planning::{ManualPowerSequenceRequest, project_manual_power_sequence};

const SETTLEMENT_DIRECT_HORIZON_BATCHES: u64 = 20;
// Keep the maintained mechanized witness materially beyond the current crossover instead of
// pinning the harness to a barely-positive order. Organic worlds derive their demand from the
// live production crossover below, so content retuning cannot silently move every sample to one
// side of the investment decision.
const SETTLEMENT_MECHANIZE_HORIZON_BATCHES: u64 = 64;
const SETTLEMENT_CROSSOVER_SEARCH_MAX_BATCHES: u64 = 128;
const SETTLEMENT_OPPORTUNITY_BATCHES: u64 =
    SETTLEMENT_CROSSOVER_SEARCH_MAX_BATCHES + SETTLEMENT_CROSSOVER_SEARCH_MAX_BATCHES / 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LumberInvestmentChoice {
    FrameSaw,
    SashSawmill,
}

fn investment_policy(case: FocusedProbeCase) -> CapitalInvestmentPolicy {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage => {
            CapitalInvestmentPolicy::baseline()
        }
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => case
            .behavior_seed()
            .map(CapitalInvestmentPolicy::from_behavior_seed)
            .unwrap_or_else(CapitalInvestmentPolicy::baseline),
    }
}

impl LumberInvestmentChoice {
    const fn label(self) -> &'static str {
        match self {
            Self::FrameSaw => "frame-saw",
            Self::SashSawmill => "sash-sawmill",
        }
    }
}

#[derive(Clone, Copy)]
struct SetupPlan {
    process: ProcessId,
    batches: u64,
    equipment: Option<EquipmentId>,
}

fn declared_lumber_batches(case: FocusedProbeCase, baseline_crossover_batches: u64) -> u64 {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor => SETTLEMENT_DIRECT_HORIZON_BATCHES,
        FocusedProbeRole::MaintainedCoverage => SETTLEMENT_MECHANIZE_HORIZON_BATCHES,
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            let spread = (baseline_crossover_batches / 2).max(1);
            let lower = baseline_crossover_batches.saturating_sub(spread).max(1);
            let upper = baseline_crossover_batches
                .checked_add(spread)
                .unwrap_or_else(|| panic!("settlement organic order range overflowed"));
            lower + mix64(case.seed() ^ 0x5345_5454_4C55_4D42) % (upper - lower + 1)
        }
    }
}

fn add_requirement(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) {
    let entry = requirements.entry(commodity).or_insert(Mass::ZERO);
    *entry = entry
        .checked_add(mass)
        .unwrap_or_else(|| panic!("settlement {context} material requirement overflowed"));
}

fn seed_prior_workshop(
    registries: &Registries,
    state: &mut AppState,
) -> deep_hearth::inventory::StockpileId {
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    for equipment in [EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_STONE_HAND_CRANK] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| panic!("settlement prior equipment lost assembly profile"));
        for input in profile.inputs() {
            add_requirement(
                &mut requirements,
                input.commodity(),
                input.mass(),
                "prior-workshop",
            );
        }
    }
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("settlement prior flywheel lost assembly profile"));
    for input in drive_profile.inputs() {
        add_requirement(
            &mut requirements,
            input.commodity(),
            input.mass(),
            "prior-workshop",
        );
    }
    let capacity = requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("settlement prior-workshop capacity overflowed"));
    let stockpile = seed_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in requirements {
        seed_lot(
            registries,
            state,
            stockpile,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    stockpile
}

fn settlement_upgrade_raw_requirements(registries: &Registries) -> BTreeMap<CommodityKey, Mass> {
    let additions = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_SASH_SAWMILL)
        .and_then(|definition| definition.upgrade_profile())
        .map(|upgrade| upgrade.additions())
        .unwrap_or_else(|| panic!("settlement sash sawmill lost upgrade additions"));
    let primitive_roots = [
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ];
    let mut requirements = BTreeMap::new();
    for input in additions.inputs() {
        let (craft, batches) = manual_craft_topology_plan_for_output_from_inputs(
            registries,
            input.commodity(),
            input.mass(),
            &primitive_roots,
            "settlement sash-sawmill raw opportunity",
        );
        let consumed = Mass::from_milligrams(
            craft
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("settlement upgrade raw requirement overflowed")),
        );
        add_requirement(
            &mut requirements,
            craft.input(),
            consumed,
            "upgrade-opportunity",
        );
    }
    requirements
}

fn setup_plans(
    registries: &Registries,
    state: &AppState,
    raw: deep_hearth::inventory::StockpileId,
    frame_saw: EquipmentId,
) -> (Vec<SetupPlan>, u64) {
    let additions = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_SASH_SAWMILL)
        .and_then(|definition| definition.upgrade_profile())
        .map(|upgrade| upgrade.additions())
        .unwrap_or_else(|| panic!("settlement sash sawmill lost upgrade additions"));
    let mut plans = Vec::new();
    let mut attention = 0_u64;
    for input in additions.inputs() {
        let candidates = registries
            .crafting()
            .manual_producers(input.commodity())
            .flat_map(|definition| {
                let per_batch = definition
                    .outputs()
                    .iter()
                    .find(|output| output.commodity() == input.commodity())
                    .map(|output| output.mass())
                    .unwrap_or_else(|| {
                        panic!(
                            "settlement sawmill producer {} lost requested output {}",
                            definition.process().value(),
                            input.commodity().value()
                        )
                    });
                let batches = input.mass().milligrams().div_ceil(per_batch.milligrams());
                let required_input = Mass::from_milligrams(
                    definition
                        .input_mass()
                        .milligrams()
                        .checked_mul(batches)
                        .unwrap_or_else(|| panic!("settlement sawmill setup input overflowed")),
                );
                [None, Some(frame_saw)]
                    .into_iter()
                    .filter_map(move |equipment| {
                        let mut request = plan_manual_craft_request(
                            registries,
                            state,
                            definition.process(),
                            raw,
                            batches,
                        )
                        .ok()?;
                        if let Some(equipment) = equipment {
                            request = request.with_equipment(equipment);
                        }
                        let resolution = resolve_manual_craft(registries, state, &request).ok()?;
                        Some((
                            definition.process(),
                            batches,
                            equipment,
                            resolution.duration().value(),
                            required_input.milligrams(),
                        ))
                    })
            })
            .collect::<Vec<_>>();
        let best_key = candidates
            .iter()
            .map(|(_, _, _, ticks, input_mg)| (*ticks, *input_mg))
            .min()
            .unwrap_or_else(|| {
                panic!(
                    "settlement sawmill setup has no legal route to component {} from disclosed raw matter",
                    input.commodity().value()
                )
            });
        let mut best = candidates
            .into_iter()
            .filter(|(_, _, _, ticks, input_mg)| (*ticks, *input_mg) == best_key);
        let (process, batches, equipment, ticks, _) = best
            .next()
            .unwrap_or_else(|| unreachable!("settlement best setup key came from a candidate"));
        assert!(
            best.next().is_none(),
            "settlement sawmill setup has equally efficient routes to component {}; add an explicit actor preference",
            input.commodity().value()
        );
        attention = attention
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("settlement sawmill setup attention overflowed"));
        plans.push(SetupPlan {
            process,
            batches,
            equipment,
        });
    }
    (plans, attention)
}

fn execute_setup(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
    plans: &[SetupPlan],
) -> u64 {
    plans
        .iter()
        .map(|plan| {
            let mut request = select_manual_craft_request(
                registries,
                state,
                plan.process,
                raw,
                plan.batches,
                "settlement sawmill setup execution",
            );
            if let Some(equipment) = plan.equipment {
                request = request.with_equipment(equipment);
            }
            execute_manual_craft(
                registries,
                state,
                request,
                parts,
                "settlement sawmill setup execution",
            )
            .value()
        })
        .try_fold(0_u64, u64::checked_add)
        .unwrap_or_else(|| panic!("settlement executed setup attention overflowed"))
}

fn baseline_lumber_crossover_batches(
    registries: &Registries,
    state: &AppState,
    raw: deep_hearth::inventory::StockpileId,
    frame_saw: EquipmentId,
    crank: EquipmentId,
    work_per_batch: deep_hearth::core::quantity::Energy,
) -> u64 {
    let (_, setup_attention) = setup_plans(registries, state, raw, frame_saw);
    let manual_process = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .and_then(|powered| registries.crafting().get_manual(powered.transform()))
        .map(|manual| manual.process())
        .unwrap_or_else(|| panic!("settlement sawmill lost its manual transform"));
    let frame_condition = state
        .equipment()
        .get_equipment(frame_saw)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("settlement frame saw disappeared before crossover planning"));
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("settlement hand crank disappeared before crossover planning"));
    let minimum_attention_return =
        CapitalInvestmentPolicy::baseline().minimum_attention_return(0, setup_attention);

    for batches in 1..=SETTLEMENT_CROSSOVER_SEARCH_MAX_BATCHES {
        let batches_nonzero = NonZeroU64::new(batches).unwrap_or_else(|| {
            unreachable!("positive settlement crossover batch count is nonzero")
        });
        let baseline_attention = project_manual_craft_equipment(
            registries,
            manual_process,
            batches_nonzero,
            EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            frame_condition,
        )
        .unwrap_or_else(|error| panic!("settlement frame-saw crossover projection failed: {error}"))
        .duration()
        .value();
        let machine_attention = setup_attention
            .checked_add(
                project_manual_power_sequence(
                    registries,
                    ManualPowerSequenceRequest {
                        method: MANUAL_POWER_HAND_CRANK,
                        equipment: EQUIPMENT_STONE_HAND_CRANK,
                        starting_condition: crank_condition,
                        store: ENERGY_STONE_FLYWHEEL_DRIVE,
                        energy_per_charge: work_per_batch,
                        charges: batches,
                    },
                    "settlement sawmill crossover",
                )
                .attention_ticks,
            )
            .unwrap_or_else(|| panic!("settlement crossover machine attention overflowed"));
        if clears_attention_return(
            baseline_attention,
            machine_attention,
            minimum_attention_return,
        ) {
            return batches;
        }
    }
    panic!(
        "settlement sawmill investment has no baseline crossover within {SETTLEMENT_CROSSOVER_SEARCH_MAX_BATCHES} batches"
    );
}

pub(super) fn run_settlement_probe(registries: &Registries, case: FocusedProbeCase) {
    let investment_policy = investment_policy(case);
    let batch = authored_batch(
        registries,
        PROCESS_POWER_SAW_WOOD_BOARDS,
        "settlement lumber investment",
    );
    let opportunity_mass = Mass::from_milligrams(
        batch
            .input_mass
            .milligrams()
            .checked_mul(SETTLEMENT_OPPORTUNITY_BATCHES)
            .unwrap_or_else(|| panic!("settlement lumber opportunity mass overflowed")),
    );
    let mut state = AppState::new();
    let bootstrap = seed_prior_workshop(registries, &mut state);
    let upgrade_requirements = settlement_upgrade_raw_requirements(registries);
    let upgrade_wood_mass = upgrade_requirements
        .get(&CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .copied()
        .unwrap_or(Mass::ZERO);
    let upgrade_copper_mass = upgrade_requirements
        .get(&CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
        .copied()
        .unwrap_or(Mass::ZERO);
    let upgrade_opportunity_capacity = upgrade_requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("settlement upgrade opportunity overflowed"));
    let upgrade_raw = seed_stockpile(
        &mut state,
        upgrade_opportunity_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in upgrade_requirements {
        seed_lot(
            registries,
            &mut state,
            upgrade_raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let upgrade_parts = seed_stockpile(
        &mut state,
        upgrade_opportunity_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let work_source = seed_stockpile(
        &mut state,
        opportunity_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_lot(
        registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        opportunity_mass,
        ROOM_TEMPERATURE,
    );
    let output = seed_stockpile(
        &mut state,
        opportunity_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    super::world_admission::locate_stationary_endpoints(
        &mut state,
        &[bootstrap, upgrade_raw, upgrade_parts, work_source, output],
        &[],
    );
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("settlement survival setup failed: {error}"));
    super::world_admission::initialize_stationary_player_logistics(&mut state);

    let frame_saw = validate_assemble_equipment(
        registries,
        &state,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("settlement frame-saw assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement frame-saw commit failed: {error}"));
    let crank =
        validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("settlement hand-crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("settlement hand-crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("settlement flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("settlement flywheel commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "settlement disclosed prior-workshop package must be exact"
    );

    let baseline_crossover_batches = baseline_lumber_crossover_batches(
        registries,
        &state,
        upgrade_raw,
        frame_saw,
        crank,
        batch.work,
    );
    let order_batches = declared_lumber_batches(case, baseline_crossover_batches);
    assert!(
        order_batches <= SETTLEMENT_OPPORTUNITY_BATCHES,
        "settlement declared order exceeds disclosed lumber opportunity"
    );
    let order_mass = Mass::from_milligrams(
        batch
            .input_mass
            .milligrams()
            .checked_mul(order_batches)
            .unwrap_or_else(|| panic!("settlement lumber order mass overflowed")),
    );
    let baseline_request = select_manual_craft_request(
        registries,
        &state,
        registries
            .crafting()
            .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
            .and_then(|powered| registries.crafting().get_manual(powered.transform()))
            .map(|manual| manual.process())
            .unwrap_or_else(|| panic!("settlement sawmill lost its manual transform")),
        work_source,
        order_batches,
        "settlement frame-saw baseline",
    )
    .with_equipment(frame_saw);
    let baseline_attention = resolve_manual_craft(registries, &state, &baseline_request)
        .unwrap_or_else(|error| panic!("settlement frame-saw projection failed: {error}"))
        .duration()
        .value();
    let (plans, setup_attention) = setup_plans(registries, &state, upgrade_raw, frame_saw);
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("settlement hand crank disappeared before investment decision"));
    let charge_projection = project_manual_power_sequence(
        registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_STONE_HAND_CRANK,
            starting_condition: crank_condition,
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: batch.work,
            charges: order_batches,
        },
        "settlement sawmill workload",
    );
    let charge_ticks = charge_projection.first_charge_ticks;
    let machine_attention = setup_attention
        .checked_add(charge_projection.attention_ticks)
        .unwrap_or_else(|| panic!("settlement machine attention overflowed"));
    let minimum_attention_return = investment_policy.minimum_attention_return(0, setup_attention);
    let choice = if clears_attention_return(
        baseline_attention,
        machine_attention,
        minimum_attention_return,
    ) {
        LumberInvestmentChoice::SashSawmill
    } else {
        LumberInvestmentChoice::FrameSaw
    };

    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("settlement matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("settlement player survival disappeared"));
    let started_at = state.tick().value();
    let mut delegated_ticks = 0_u64;
    let mut upgraded = false;
    let active_attention = match choice {
        LumberInvestmentChoice::FrameSaw => execute_manual_craft(
            registries,
            &mut state,
            baseline_request,
            output,
            "settlement frame-saw order",
        )
        .value(),
        LumberInvestmentChoice::SashSawmill => {
            let executed_setup =
                execute_setup(registries, &mut state, upgrade_raw, upgrade_parts, &plans);
            assert_eq!(
                executed_setup, setup_attention,
                "settlement setup projection must match execution"
            );
            let mut active_attention = executed_setup;
            let sawmill = validate_upgrade_equipment(
                registries,
                &state,
                frame_saw,
                EQUIPMENT_TIMBER_SASH_SAWMILL,
                upgrade_parts,
            )
            .unwrap_or_else(|error| panic!("settlement sash-sawmill upgrade failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("settlement sash-sawmill commit failed: {error}"));
            assert_eq!(
                sawmill, frame_saw,
                "settlement upgrade must preserve equipment identity"
            );
            upgraded = true;

            for _ in 0..order_batches {
                let power = validate_start_manual_power(
                    registries,
                    &state,
                    ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, batch.work),
                )
                .unwrap_or_else(|error| panic!("settlement sawmill charging failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("settlement sawmill charge commit failed: {error}"));
                active_attention = active_attention
                    .checked_add(finish_manual_power_work(
                        registries,
                        &mut state,
                        power,
                        "settlement sawmill charging",
                    ))
                    .unwrap_or_else(|| panic!("settlement active attention overflowed"));
                let selections = select_stockpile_mass(
                    &state,
                    work_source,
                    batch.input_mass,
                    "settlement sawmill feed",
                );
                let job = validate_start_powered_craft(
                    registries,
                    &state,
                    PoweredCraftRequest::new(
                        PROCESS_POWER_SAW_WOOD_BOARDS,
                        work_source,
                        selections,
                        sawmill,
                        drive,
                    ),
                    output,
                )
                .unwrap_or_else(|error| panic!("settlement sawmill start failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("settlement sawmill commit failed: {error}"));
                let duration = state
                    .production()
                    .get_job(job)
                    .map(|record| record.active_duration().value())
                    .unwrap_or_else(|| panic!("settlement sawmill job disappeared"));
                delegated_ticks = delegated_ticks
                    .checked_add(duration)
                    .unwrap_or_else(|| panic!("settlement delegated time overflowed"));
                finish_uninterrupted_production_job(
                    registries,
                    &mut state,
                    job,
                    "settlement unattended sawing",
                );
            }
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(crank)
                    .map(|record| record.condition()),
                Some(charge_projection.condition_after),
                "settlement projected hand-crank wear must match executed repeated charging"
            );
            active_attention
        }
    };
    assert_eq!(
        active_attention,
        match choice {
            LumberInvestmentChoice::FrameSaw => baseline_attention,
            LumberInvestmentChoice::SashSawmill => machine_attention,
        },
        "settlement chosen-route attention must match the frozen decision projection"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("settlement final state invalid: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("settlement matter audit failed: {error}"))
            .total(),
        matter_before,
        "settlement lumber episode must conserve represented matter"
    );
    let output_record = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("settlement output stockpile disappeared"));
    let board_mass = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD));
    let chip_mass = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP));
    assert!(
        !board_mass.is_zero(),
        "settlement lumber order must produce useful boards"
    );
    let elapsed = state.tick().value() - started_at;
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("settlement player survival disappeared after order"));
    let attention_saved = i128::from(baseline_attention) - i128::from(machine_attention);
    reviewln!(
        "SETTLEMENT EXPERIENCE seed=0x{:016X} sample={} demand=[batches:{} mass:{}mg] decision=[choice:{} policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t baseline:{}t mechanized:{}t setup:{}t charging-total:{}t first-charge:{}t margin:{:+}t] execution=[active:{}t elapsed:{}t/{} delegated:{}t upgraded:{} boards:{}mg chips:{}mg] survival=[energy-spent:{}nJ hydration-spent:{}uL] prior-infrastructure=frame-saw+hand-crank+flywheel raw-upgrade-opportunity=[wood:{}mg copper:{}mg] matter=conserved",
        case.seed(),
        case.role().label(),
        order_batches,
        order_mass.milligrams(),
        choice.label(),
        investment_policy.minimum_return_ppm(),
        minimum_attention_return,
        baseline_attention,
        machine_attention,
        setup_attention,
        charge_projection.attention_ticks,
        charge_ticks,
        attention_saved,
        active_attention,
        elapsed,
        format_physical_duration(registries, elapsed),
        delegated_ticks,
        upgraded,
        board_mass.milligrams(),
        chip_mass.milligrams(),
        survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy())
            .unwrap_or_else(|| panic!("settlement metabolic reserve increased"))
            .nanojoules(),
        survival_before
            .hydration()
            .checked_sub(survival_after.hydration())
            .unwrap_or_else(|| panic!("settlement hydration reserve increased"))
            .microliters(),
        upgrade_wood_mass.milligrams(),
        upgrade_copper_mass.milligrams(),
    );
}
