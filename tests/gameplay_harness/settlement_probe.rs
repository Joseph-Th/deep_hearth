//! Ordinary settlement lumber investment episode over disclosed prior workshop infrastructure.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::content::gameplay_fixture::{
    seed_assembled_energy_store_at, seed_assembled_equipment_at, seed_lot,
    seed_preused_assembled_equipment_at, seed_stockpile,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_BOARD, FORM_CHIP, FORM_LOG, FORM_NATIVE_METAL,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_WOOD, PROCESS_POWER_SAW_WOOD_BOARDS,
    PROCESS_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    manual_craft_options_from_stockpile, project_manual_craft_equipment, resolve_manual_craft,
};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId, validate_upgrade_equipment};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;
use deep_hearth::survival::assess_survival;

use super::capital_investment_crossover::first_attention_return_crossover;
use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::exact_local_runtime::STATIONARY_PLAYER_ORIGIN;
use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::{plan_manual_craft_request, select_manual_craft_request};
use super::manual_craft_topology_planning::manual_craft_topology_plan_for_output_from_inputs;
use super::physical_time::format_physical_duration;
use super::powered_craft_planning::authored_batch;
use super::seed::mix64;
use super::settlement_generation::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES as SETTLEMENT_OPPORTUNITY_BATCHES,
    crossover_workloads, organic_inherited_equipment_condition, organic_investment_policy,
    organic_lumber_batches,
};
use super::settlement_power_planning::project_manual_power_workload;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LumberInvestmentChoice {
    FrameSaw,
    SashSawmill,
}

fn authored_mass_flow_providers(
    registries: &Registries,
    capability: CapabilityId,
) -> BTreeSet<EquipmentDefinitionId> {
    registries
        .equipment()
        .definitions()
        .filter(|definition| definition.has_authored_acquisition_edge())
        .filter(|definition| {
            matches!(
                definition.capabilities().get_capability(capability),
                Some(CapabilityValue::MassFlow(flow)) if !flow.is_zero()
            )
        })
        .map(|definition| definition.id())
        .collect()
}

fn assert_settlement_lumber_market_current(registries: &Registries) {
    let manual = registries
        .crafting()
        .get_manual(PROCESS_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement manual sawing route disappeared"));
    let manual_capability = manual
        .equipment_profile()
        .map(|profile| profile.mass_flow_capability())
        .unwrap_or_else(|| panic!("settlement manual sawing route lost its equipment requirement"));
    assert_eq!(
        authored_mass_flow_providers(registries, manual_capability),
        BTreeSet::from([
            EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            EQUIPMENT_TIMBER_SASH_SAWMILL,
        ]),
        "settlement lumber episode diverged from the current authored manual-saw provider market"
    );

    let powered_variants = registries
        .crafting()
        .powered_variants(PROCESS_SAW_WOOD_BOARDS)
        .collect::<Vec<_>>();
    let [powered] = powered_variants.as_slice() else {
        panic!(
            "settlement lumber episode expects one current powered variant of manual sawing, found {}",
            powered_variants.len()
        );
    };
    assert_eq!(
        powered.process(),
        PROCESS_POWER_SAW_WOOD_BOARDS,
        "settlement lumber episode diverged from the current powered sawing transform"
    );
    let powered_capability = powered.mass_flow_capability();
    assert_eq!(
        authored_mass_flow_providers(registries, powered_capability),
        BTreeSet::from([EQUIPMENT_TIMBER_SASH_SAWMILL]),
        "settlement lumber episode diverged from the current authored powered-saw provider market"
    );
    let upgrade_from = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_SASH_SAWMILL)
        .and_then(|definition| definition.upgrade_profile())
        .map(|upgrade| upgrade.from())
        .unwrap_or_else(|| panic!("settlement sash sawmill lost its authored upgrade route"));
    assert_eq!(
        upgrade_from, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        "settlement lumber episode no longer matches the authored frame-saw upgrade edge"
    );
}

#[path = "settlement_probe/lumber_followup.rs"]
mod lumber_followup;

fn investment_policy(case: FocusedProbeCase) -> CapitalInvestmentPolicy {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage => {
            CapitalInvestmentPolicy::baseline()
        }
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            organic_investment_policy(case.required_behavior_seed("settlement investment policy"))
        }
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

fn declared_lumber_batches(case: FocusedProbeCase, baseline_crossover_batches: Option<u64>) -> u64 {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage => {
            let crossover = baseline_crossover_batches.unwrap_or_else(|| {
                panic!(
                    "maintained settlement lumber opportunity has no live mechanization crossover"
                )
            });
            let workloads =
                crossover_workloads(crossover, SETTLEMENT_OPPORTUNITY_BATCHES - 1, None);
            match case.role() {
                FocusedProbeRole::MaintainedAnchor => workloads.short_batches,
                FocusedProbeRole::MaintainedCoverage => workloads.project_batches,
                FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
                    unreachable!("maintained settlement branch received organic role")
                }
            }
        }
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            organic_lumber_batches(
                case.seed() & 0b11,
                mix64(case.seed() ^ 0x5345_5454_4C55_4D42),
                SETTLEMENT_OPPORTUNITY_BATCHES,
            )
        }
    }
}

#[derive(Clone, Copy)]
struct SetupPlan {
    process: ProcessId,
    batches: u64,
    equipment: Option<EquipmentId>,
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

#[derive(Clone, Copy)]
struct PriorWorkshop {
    component_source: deep_hearth::inventory::StockpileId,
    frame_saw: EquipmentId,
    crank: EquipmentId,
    drive: deep_hearth::energy::EnergyStoreId,
    frame_saw_condition: Condition,
    crank_condition: Condition,
}

fn inherited_equipment_condition(
    registries: &Registries,
    definition: deep_hearth::equipment::EquipmentDefinitionId,
    case: FocusedProbeCase,
) -> Condition {
    if matches!(
        case.role(),
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage
    ) {
        return Condition::PRISTINE;
    }
    organic_inherited_equipment_condition(
        registries,
        definition,
        mix64(case.seed() ^ u64::from(definition.value()) ^ 0x5345_5454_5553_4544),
    )
}

fn seed_prior_workshop(
    registries: &Registries,
    state: &mut AppState,
    case: FocusedProbeCase,
) -> PriorWorkshop {
    let component_source = super::settlement_fixture::seed_inherited_workshop_package(
        registries,
        state,
        &[EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_STONE_HAND_CRANK],
        &[ENERGY_STONE_FLYWHEEL_DRIVE],
        &[],
        "focused settlement prior workshop",
    );
    let frame_saw_condition =
        inherited_equipment_condition(registries, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, case);
    let crank_condition =
        inherited_equipment_condition(registries, EQUIPMENT_STONE_HAND_CRANK, case);
    let assemble = |state: &mut AppState, definition, condition| {
        if condition == Condition::PRISTINE {
            seed_assembled_equipment_at(
                registries,
                state,
                definition,
                component_source,
                STATIONARY_PLAYER_ORIGIN,
            )
        } else {
            seed_preused_assembled_equipment_at(
                registries,
                state,
                definition,
                component_source,
                STATIONARY_PLAYER_ORIGIN,
                condition,
            )
        }
    };
    let frame_saw = assemble(state, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, frame_saw_condition);
    let crank = assemble(state, EQUIPMENT_STONE_HAND_CRANK, crank_condition);
    let drive = seed_assembled_energy_store_at(
        registries,
        state,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        component_source,
        STATIONARY_PLAYER_ORIGIN,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(component_source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "settlement inherited workshop must embody its complete disclosed component stock"
    );
    PriorWorkshop {
        component_source,
        frame_saw,
        crank,
        drive,
        frame_saw_condition,
        crank_condition,
    }
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
        let catalog =
            manual_craft_options_from_stockpile(registries, state, raw).unwrap_or_else(|error| {
                panic!("settlement cannot read the ordinary craft catalog: {error}")
            });
        let candidates = catalog
            .into_iter()
            .filter_map(|option| {
                let definition = registries
                    .crafting()
                    .get_manual(option.process())
                    .unwrap_or_else(|| panic!("ordinary craft catalog exposed an unknown process"));
                if !definition
                    .outputs()
                    .iter()
                    .any(|output| output.commodity() == input.commodity())
                {
                    return None;
                }
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
                Some(
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
                            let resolution =
                                resolve_manual_craft(registries, state, &request).ok()?;
                            Some((
                                definition.process(),
                                batches,
                                equipment,
                                resolution.duration().value(),
                                required_input.milligrams(),
                            ))
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
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
    drive: deep_hearth::energy::EnergyStoreId,
    batch: super::powered_craft_planning::AuthoredPoweredCraftBatch,
) -> Option<u64> {
    let (plans, setup_attention) = setup_plans(registries, state, raw, frame_saw);
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
    let sawmill_start_condition = lumber_followup::project_setup_equipment_condition(
        registries,
        &plans,
        frame_saw,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        frame_condition,
    );
    let minimum_attention_return =
        CapitalInvestmentPolicy::baseline().minimum_attention_return(0, setup_attention);

    first_attention_return_crossover(
        SETTLEMENT_OPPORTUNITY_BATCHES - 1,
        minimum_attention_return,
        |batches| {
            let batches_nonzero = NonZeroU64::new(batches)
                .unwrap_or_else(|| unreachable!("positive settlement batch count is nonzero"));
            let baseline_attention = project_manual_craft_equipment(
                registries,
                manual_process,
                batches_nonzero,
                EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
                frame_condition,
            )
            .ok()?
            .duration()
            .value();
            let powered = lumber_followup::project_future_powered_lumber_sequence(
                registries,
                state,
                EQUIPMENT_TIMBER_SASH_SAWMILL,
                sawmill_start_condition,
                drive,
                batch,
                batches,
            );
            if powered.batches != batches {
                return None;
            }
            let machine_attention = setup_attention.checked_add(
                project_manual_power_workload(
                    registries,
                    MANUAL_POWER_HAND_CRANK,
                    EQUIPMENT_STONE_HAND_CRANK,
                    crank_condition,
                    ENERGY_STONE_FLYWHEEL_DRIVE,
                    powered.charge_energies(),
                    "settlement sawmill crossover",
                )
                .attention_ticks,
            )?;
            Some((baseline_attention, machine_attention))
        },
    )
}

pub(super) fn run_settlement_probe(registries: &Registries, case: FocusedProbeCase) {
    assert_settlement_lumber_market_current(registries);
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
    let prior_workshop = seed_prior_workshop(registries, &mut state, case);
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
    super::world_admission::admit_stationary_player(
        registries,
        &mut state,
        &[
            prior_workshop.component_source,
            upgrade_raw,
            upgrade_parts,
            work_source,
            output,
        ],
        &[],
        "focused settlement",
    );
    let frame_saw = prior_workshop.frame_saw;
    let crank = prior_workshop.crank;
    let drive = prior_workshop.drive;

    let baseline_crossover_batches = baseline_lumber_crossover_batches(
        registries,
        &state,
        upgrade_raw,
        frame_saw,
        crank,
        drive,
        batch,
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
        .unwrap_or_else(|| panic!("settlement frame saw disappeared before investment decision"));
    let manual_capacity = lumber_followup::maximum_feasible_batches(order_batches, |batches| {
        let batches = NonZeroU64::new(batches)
            .unwrap_or_else(|| unreachable!("positive settlement batch count is nonzero"));
        project_manual_craft_equipment(
            registries,
            manual_process,
            batches,
            EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            frame_condition,
        )
        .is_ok()
    });
    let baseline_attention = NonZeroU64::new(manual_capacity)
        .map(|batches| {
            project_manual_craft_equipment(
                registries,
                manual_process,
                batches,
                EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
                frame_condition,
            )
            .unwrap_or_else(|error| {
                panic!("settlement feasible frame-saw projection failed: {error}")
            })
            .duration()
            .value()
        })
        .unwrap_or(0);
    let (plans, setup_attention) = setup_plans(registries, &state, upgrade_raw, frame_saw);
    let sawmill_start_condition = lumber_followup::project_setup_equipment_condition(
        registries,
        &plans,
        frame_saw,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        frame_condition,
    );
    let powered_projection = lumber_followup::project_future_powered_lumber_sequence(
        registries,
        &state,
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        sawmill_start_condition,
        drive,
        batch,
        order_batches,
    );
    let powered_capacity = powered_projection.batches;
    assert!(
        powered_capacity > 0,
        "settlement sash-sawmill upgrade cannot execute even one disclosed lumber batch"
    );
    assert!(
        manual_capacity == order_batches || powered_capacity == order_batches,
        "settlement disclosed order has no complete route: frame-saw capacity {manual_capacity}/{order_batches}, sash-sawmill capacity {powered_capacity}/{order_batches}"
    );
    let crank_condition = state
        .equipment()
        .get_equipment(crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("settlement hand crank disappeared before investment decision"));
    let charge_projection = project_manual_power_workload(
        registries,
        MANUAL_POWER_HAND_CRANK,
        EQUIPMENT_STONE_HAND_CRANK,
        crank_condition,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        powered_projection.charge_energies(),
        "settlement sawmill workload",
    );
    let charge_ticks = charge_projection.first_charge_ticks;
    let machine_attention = setup_attention
        .checked_add(charge_projection.attention_ticks)
        .unwrap_or_else(|| panic!("settlement machine attention overflowed"));
    let minimum_attention_return = investment_policy.minimum_attention_return(0, setup_attention);
    let (choice, decision_basis) = if powered_capacity > manual_capacity {
        (LumberInvestmentChoice::SashSawmill, "capacity")
    } else if manual_capacity > powered_capacity {
        (LumberInvestmentChoice::FrameSaw, "capacity")
    } else if clears_attention_return(
        baseline_attention,
        machine_attention,
        minimum_attention_return,
    ) {
        (LumberInvestmentChoice::SashSawmill, "attention-return")
    } else {
        (LumberInvestmentChoice::FrameSaw, "attention-return")
    };

    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("settlement matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("settlement player survival disappeared"));
    let started_at = state.tick().value();
    let mut delegated_ticks = 0_u64;
    let mut charge_events = 0_u64;
    let mut upgraded = false;
    let active_attention = match choice {
        LumberInvestmentChoice::FrameSaw => {
            assert_eq!(
                manual_capacity, order_batches,
                "settlement selected frame saw must be able to finish the disclosed order"
            );
            let request = select_manual_craft_request(
                registries,
                &state,
                manual_process,
                work_source,
                order_batches,
                "settlement frame-saw order",
            )
            .with_equipment(frame_saw);
            execute_manual_craft(
                registries,
                &mut state,
                request,
                output,
                "settlement frame-saw order",
            )
            .value()
        }
        LumberInvestmentChoice::SashSawmill => {
            assert_eq!(
                powered_capacity, order_batches,
                "settlement selected sash sawmill must be able to finish the disclosed order"
            );
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

            let (charging_attention, delegated, executed_charge_events) =
                lumber_followup::execute_powered_lumber_order(
                    registries,
                    &mut state,
                    lumber_followup::PoweredLumberOrder {
                        source: work_source,
                        output,
                        sawmill,
                        crank,
                        drive,
                        batch,
                        batches: order_batches,
                        context: "settlement sawmill order",
                    },
                );
            active_attention = active_attention
                .checked_add(charging_attention)
                .unwrap_or_else(|| panic!("settlement active attention overflowed"));
            delegated_ticks = delegated;
            charge_events = executed_charge_events;
            assert_eq!(
                charge_events,
                powered_projection.charge_events(),
                "settlement packed charge plan must match execution"
            );
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
    let first_elapsed = state.tick().value() - started_at;
    let remaining_opportunity_batches = SETTLEMENT_OPPORTUNITY_BATCHES
        .checked_sub(order_batches)
        .unwrap_or_else(|| unreachable!("declared settlement order fits disclosed opportunity"));
    let followup_batches = order_batches.min(remaining_opportunity_batches);
    assert!(
        followup_batches > 0,
        "settlement lived episode must leave a disclosed follow-up lumber opportunity"
    );
    let initially_upgraded = upgraded;
    let followup = lumber_followup::run_lumber_followup(lumber_followup::LumberFollowupInputs {
        registries,
        state: &mut state,
        investment_policy,
        batch,
        manual_process,
        work_source,
        output,
        upgrade_raw,
        upgrade_parts,
        frame_saw,
        crank,
        drive,
        demand_batches: followup_batches,
        choice,
        board_mass_before: board_mass,
        chip_mass_before: chip_mass,
    });
    upgraded = followup.final_upgraded;
    let followup_active_attention = followup.active_attention;
    let followup_delegated_ticks = followup.delegated_ticks;
    let followup_charge_events = followup.charge_events;
    let followup_route = followup.route;
    let followup_reinvested = followup.reinvested;
    let followup_completed_batches = followup.completed_batches;
    let followup_reassessment = followup.reassessment;
    let followup_elapsed = followup.elapsed_ticks;
    let followup_board_mass = followup.board_mass;
    let followup_chip_mass = followup.chip_mass;
    let followup_terminal = followup.terminal;
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("settlement follow-up state invalid: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("settlement follow-up matter audit failed: {error}"))
            .total(),
        matter_before,
        "settlement follow-up must conserve represented matter"
    );
    let episode_elapsed = state.tick().value() - started_at;
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("settlement player survival disappeared after order"));
    let attention_saved = i128::from(baseline_attention) - i128::from(machine_attention);
    let baseline_crossover =
        baseline_crossover_batches.map_or_else(|| "none".to_owned(), |batches| batches.to_string());
    reviewln!(
        "SETTLEMENT EXPERIENCE seed=0x{:016X} sample={} demand=[batches:{} mass:{}mg baseline-crossover:{}] decision=[choice:{} basis:{} capacity=[frame-saw:{}/{} sash-sawmill:{}/{}] power-cycle=[charges:{} max-batches-per-charge:{}] policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t baseline:{}t mechanized:{}t setup:{}t charging-total:{}t first-charge:{}t margin:{:+}t followup-not-input:true] execution=[active:{}t elapsed:{}t/{} delegated:{}t charge-events:{} upgraded:{} boards:{}mg chips:{}mg] followup=[demand-batches:{} completed:{} terminal:{} route:{} active:{}t elapsed:{}t/{} delegated:{}t charge-events:{} machine-owned-before:{} reinvested:{} boards-total:{}mg chips-total:{}mg] followup-reassessment=[{}] episode=[elapsed:{}t/{} upgraded-final:{}] survival=[energy-spent:{}nJ hydration-spent:{}uL] prior-infrastructure=[frame-saw-condition:{}ppm crank-condition:{}ppm flywheel=stone prior-use=pre-existing] raw-upgrade-opportunity=[wood:{}mg copper:{}mg] matter=conserved",
        case.seed(),
        case.role().label(),
        order_batches,
        order_mass.milligrams(),
        baseline_crossover,
        choice.label(),
        decision_basis,
        manual_capacity,
        order_batches,
        powered_capacity,
        order_batches,
        powered_projection.charge_events(),
        powered_projection.maximum_leg_batches(),
        investment_policy.minimum_return_ppm(),
        minimum_attention_return,
        baseline_attention,
        machine_attention,
        setup_attention,
        charge_projection.attention_ticks,
        charge_ticks,
        attention_saved,
        active_attention,
        first_elapsed,
        format_physical_duration(registries, first_elapsed),
        delegated_ticks,
        charge_events,
        initially_upgraded,
        board_mass.milligrams(),
        chip_mass.milligrams(),
        followup_batches,
        followup_completed_batches,
        followup_terminal,
        followup_route,
        followup_active_attention,
        followup_elapsed,
        format_physical_duration(registries, followup_elapsed),
        followup_delegated_ticks,
        followup_charge_events,
        initially_upgraded,
        followup_reinvested,
        followup_board_mass.milligrams(),
        followup_chip_mass.milligrams(),
        followup_reassessment,
        episode_elapsed,
        format_physical_duration(registries, episode_elapsed),
        upgraded,
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
        prior_workshop.frame_saw_condition.parts_per_million(),
        prior_workshop.crank_condition.parts_per_million(),
        upgrade_wood_mass.milligrams(),
        upgrade_copper_mass.milligrams(),
    );
}
