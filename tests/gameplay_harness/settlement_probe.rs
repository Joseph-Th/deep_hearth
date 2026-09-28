//! Ordinary settlement lumber investment episode over disclosed prior workshop infrastructure.

use std::collections::BTreeMap;

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_BOARD, FORM_CHIP, FORM_LOG, FORM_NATIVE_METAL,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_WOOD, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    PoweredCraftRequest, resolve_manual_craft, validate_start_powered_craft,
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

use super::capital_investment_policy::{clears_attention_return, minimum_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::physical_time::format_physical_duration;
use super::powered_craft_planning::authored_batch;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;

const SETTLEMENT_DIRECT_HORIZON_BATCHES: u64 = 20;
const SETTLEMENT_MECHANIZE_HORIZON_BATCHES: u64 = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LumberInvestmentChoice {
    FrameSaw,
    SashSawmill,
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

fn declared_lumber_batches(case: FocusedProbeCase) -> u64 {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor => SETTLEMENT_DIRECT_HORIZON_BATCHES,
        FocusedProbeRole::MaintainedCoverage => SETTLEMENT_MECHANIZE_HORIZON_BATCHES,
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            16 + mix64(case.seed() ^ 0x5345_5454_4C55_4D42) % 25
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
        let raw_stockpile = state
            .inventory()
            .get_stockpile(raw)
            .unwrap_or_else(|| panic!("settlement sawmill raw opportunity disappeared"));
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
                let has_input = raw_stockpile.get_mass(definition.input()) >= required_input;
                [None, Some(frame_saw)]
                    .into_iter()
                    .filter_map(move |equipment| {
                        if !has_input {
                            return None;
                        }
                        let mut request = select_manual_craft_request(
                            registries,
                            state,
                            definition.process(),
                            raw,
                            batches,
                            "settlement sawmill setup projection",
                        );
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

pub(super) fn run_settlement_probe(registries: &Registries, case: FocusedProbeCase) {
    let batch = authored_batch(
        registries,
        PROCESS_POWER_SAW_WOOD_BOARDS,
        "settlement lumber investment",
    );
    let order_batches = declared_lumber_batches(case);
    let order_mass = Mass::from_milligrams(
        batch
            .input_mass
            .milligrams()
            .checked_mul(order_batches)
            .unwrap_or_else(|| panic!("settlement lumber order mass overflowed")),
    );
    let mut state = AppState::new();
    let bootstrap = seed_prior_workshop(registries, &mut state);
    let upgrade_raw = seed_stockpile(
        &mut state,
        Mass::from_milligrams(10_200_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_lot(
        registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(200_000),
        ROOM_TEMPERATURE,
    );
    let upgrade_parts = seed_stockpile(
        &mut state,
        Mass::from_milligrams(12_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let work_source = seed_stockpile(
        &mut state,
        order_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_lot(
        registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        order_mass,
        ROOM_TEMPERATURE,
    );
    let output = seed_stockpile(
        &mut state,
        order_mass,
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
    let charge = validate_start_manual_power(
        registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, batch.work),
    )
    .unwrap_or_else(|error| panic!("settlement sawmill charge projection failed: {error}"));
    let charge_ticks = charge.work().completes_at().value() - state.tick().value();
    let machine_attention = setup_attention
        .checked_add(
            charge_ticks
                .checked_mul(order_batches)
                .unwrap_or_else(|| panic!("settlement repeated charge attention overflowed")),
        )
        .unwrap_or_else(|| panic!("settlement machine attention overflowed"));
    let minimum_attention_return = minimum_attention_return(0, setup_attention);
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
        "SETTLEMENT EXPERIENCE seed=0x{:016X} sample={} demand=[batches:{} mass:{}mg] decision=[choice:{} policy=attention-first-with-minimum-investment-return minimum-attention-return:{}t baseline:{}t mechanized:{}t setup:{}t charge-per-batch:{}t margin:{:+}t] execution=[active:{}t elapsed:{}t/{} delegated:{}t upgraded:{} boards:{}mg chips:{}mg] survival=[energy-spent:{}nJ hydration-spent:{}uL] prior-infrastructure=frame-saw+hand-crank+flywheel raw-upgrade-opportunity=[wood:10000000mg copper:200000mg] matter=conserved",
        case.seed(),
        case.role().label(),
        order_batches,
        order_mass.milligrams(),
        choice.label(),
        minimum_attention_return,
        baseline_attention,
        machine_attention,
        setup_attention,
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
    );
}
