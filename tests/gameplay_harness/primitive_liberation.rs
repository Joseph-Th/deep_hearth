//! Ordinary primitive liberation episode shared by progression gates and exploratory reports.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::gameplay_fixture::seed_composed_lot;
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK,
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_STONE_ROTARY_QUERN,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER,
    PROCESS_REGRIND_COPPER_TAILINGS, PROCESS_SCAVENGE_COPPER_TAILINGS, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::{Mass, Power};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{EnergyCarrier, EnergyStoreId};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::inventory::{MaterialLotId, StockpileId};
use deep_hearth::labor::ManualPowerMethodId;
use deep_hearth::logistics::validate_allocate_ground_stockpile;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    project_powered_ore_replenished_batch_capacity, resolve_representable_screening_mass,
};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::survival::assess_survival;

use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::focused_witnesses::LIBERATION_MANUAL_FALLBACK_COVERAGE_SEED;
use super::inherited_condition::healthy_used_equipment_condition;
use super::manual_ore_recovery::{ManualOreRecoveryPlan, execute_manual_ore_recovery};
use super::manual_ore_recovery_evaluation::{
    evaluate_manual_ore_recovery, project_manual_ore_recovery_attention,
};
use super::ore_fixture::copper_ore_composition;
use super::seed::mix64;
use super::settlement_power_planning::{ManualPowerSequenceRequest, project_manual_power_sequence};

fn ordinary_manual_electrical_power_providers(
    registries: &Registries,
) -> Vec<(ManualPowerMethodId, EquipmentDefinitionId, Power)> {
    registries
        .labor()
        .manual_power_definitions()
        .filter(|method| method.carrier() == EnergyCarrier::Electrical)
        .flat_map(|method| {
            registries
                .equipment()
                .definitions()
                .filter(|equipment| equipment.has_authored_acquisition_edge())
                .filter_map(move |equipment| {
                    match equipment
                        .capabilities()
                        .get_capability(method.power_capability())
                    {
                        Some(CapabilityValue::Power(power)) if !power.is_zero() => {
                            Some((method.id(), equipment.id(), power))
                        }
                        Some(_) | None => None,
                    }
                })
        })
        .collect()
}

#[path = "primitive_liberation/acquisition.rs"]
mod acquisition;
#[path = "primitive_liberation/cleanup.rs"]
mod cleanup;
#[path = "primitive_liberation/comparison.rs"]
mod comparison;
#[cfg(test)]
include_primitive_liberation_generation_contract_tests!();
#[path = "primitive_liberation/primary.rs"]
mod primary;
#[path = "primitive_liberation/scavenging.rs"]
mod scavenging;
#[path = "primitive_liberation/support.rs"]
mod support;

const PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES: u64 = 8;
const PRIMITIVE_LIBERATION_ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);
const LIBERATION_POWERED_ROUTE_STAGES: [deep_hearth::production::ProcessId; 8] = [
    PROCESS_CRUSH_ORE,
    PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_SCREEN_CRUSHED_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_CONCENTRATE_COPPER,
    PROCESS_REGRIND_COPPER_TAILINGS,
    PROCESS_SCAVENGE_COPPER_TAILINGS,
    PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE,
];

fn inherited_progression_condition(
    registries: &Registries,
    definition: EquipmentDefinitionId,
    case: FocusedProbeCase,
    salt: u64,
) -> Condition {
    if matches!(
        case.role(),
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage
    ) {
        return Condition::PRISTINE;
    }
    healthy_used_equipment_condition(
        registries,
        definition,
        mix64(case.seed() ^ 0x4C49_4245_494E_4845 ^ salt),
    )
}

#[derive(Clone, Copy)]
struct PrimitiveLiberationBootstrap {
    ore: StockpileId,
    crushed: StockpileId,
    ground: StockpileId,
    undersize: StockpileId,
    oversize: StockpileId,
    concentrate: StockpileId,
    tailings: StockpileId,
    fine_tailings: StockpileId,
    exhausted_tailings: StockpileId,
    native_copper: StockpileId,
    manual_crushed: StockpileId,
    manual_native: StockpileId,
    manual_residue: StockpileId,
    ore_lot: MaterialLotId,
}

fn allocate_liberation_stockpile(state: &mut AppState, capacity: Mass) -> StockpileId {
    validate_allocate_ground_stockpile(state, PRIMITIVE_LIBERATION_ORIGIN, capacity)
        .unwrap_or_else(|error| panic!("liberation stockpile allocation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("liberation stockpile allocation commit failed: {error}"))
}

fn bootstrap_liberation_inventory(
    registries: &Registries,
    state: &mut AppState,
    batch_mass: Mass,
    copper_ppm: u32,
    clay_share_ppm: u32,
) -> PrimitiveLiberationBootstrap {
    let ore = allocate_liberation_stockpile(state, batch_mass);
    let crushed = allocate_liberation_stockpile(state, batch_mass);
    let ground = allocate_liberation_stockpile(state, batch_mass);
    let undersize = allocate_liberation_stockpile(state, batch_mass);
    let oversize = allocate_liberation_stockpile(state, batch_mass);
    let concentrate = allocate_liberation_stockpile(state, batch_mass);
    let tailings = allocate_liberation_stockpile(state, batch_mass);
    let fine_tailings = allocate_liberation_stockpile(state, batch_mass);
    let exhausted_tailings = allocate_liberation_stockpile(state, batch_mass);
    let native_copper = allocate_liberation_stockpile(state, batch_mass);
    let manual_crushed = allocate_liberation_stockpile(state, batch_mass);
    let manual_native = allocate_liberation_stockpile(state, batch_mass);
    let manual_residue = allocate_liberation_stockpile(state, batch_mass);
    let ore_lot = seed_composed_lot(
        registries,
        state,
        ore,
        CommodityKey::new(MATERIAL_COPPER, deep_hearth::content::FORM_ORE),
        batch_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(copper_ppm, clay_share_ppm),
    );
    PrimitiveLiberationBootstrap {
        ore,
        crushed,
        ground,
        undersize,
        oversize,
        concentrate,
        tailings,
        fine_tailings,
        exhausted_tailings,
        native_copper,
        manual_crushed,
        manual_native,
        manual_residue,
        ore_lot,
    }
}

#[derive(Clone)]
struct PrimitiveLiberationScenario {
    charge_policy: support::ChargePolicy,
    charges: Vec<support::ChargeReport>,
    state: AppState,
    batch_mass: Mass,
    copper_ppm: u32,
    ore: deep_hearth::inventory::StockpileId,
    crushed: deep_hearth::inventory::StockpileId,
    ground: deep_hearth::inventory::StockpileId,
    undersize: deep_hearth::inventory::StockpileId,
    oversize: deep_hearth::inventory::StockpileId,
    concentrate: deep_hearth::inventory::StockpileId,
    tailings: deep_hearth::inventory::StockpileId,
    fine_tailings: deep_hearth::inventory::StockpileId,
    exhausted_tailings: deep_hearth::inventory::StockpileId,
    native_copper: deep_hearth::inventory::StockpileId,
    ore_lot: deep_hearth::inventory::MaterialLotId,
    crusher: EquipmentId,
    quern: EquipmentId,
    screen: EquipmentId,
    separator: EquipmentId,
    power_method: ManualPowerMethodId,
    power_provider: EquipmentId,
    drive: EnergyStoreId,
}

#[derive(Debug)]
struct PrimitiveLiberationCampaignLifecycle {
    batch_charge_ticks: Vec<u64>,
    elapsed_ticks: u64,
    metabolic_cost_nj: u128,
    hydration_cost_ul: u64,
    recovered_native: Mass,
    crusher_condition_ppm: u32,
    quern_condition_ppm: u32,
    screen_condition_ppm: u32,
    separator_condition_ppm: u32,
    power_provider_condition_ppm: u32,
}

#[derive(Debug)]
struct ManualLiberationCampaignLifecycle {
    attention_ticks: u64,
    elapsed_ticks: u64,
    metabolic_cost_nj: u128,
    hydration_cost_ul: u64,
    recovered_native: Mass,
}

#[derive(Clone, Copy)]
struct PrimitiveLiberationCampaignInfrastructure {
    crusher: EquipmentId,
    quern: EquipmentId,
    screen: EquipmentId,
    separator: EquipmentId,
    power_method: ManualPowerMethodId,
    power_provider: EquipmentId,
    drive: EnergyStoreId,
}

fn run_powered_campaign_lifecycle(
    registries: &Registries,
    mut state: AppState,
    batches: &[PrimitiveLiberationBootstrap],
    infrastructure: PrimitiveLiberationCampaignInfrastructure,
) -> PrimitiveLiberationCampaignLifecycle {
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("liberation campaign matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation campaign player disappeared before execution"));
    let started_at = state.tick().value();
    let mut batch_charge_ticks = Vec::with_capacity(batches.len());
    let mut recovered_native = Mass::ZERO;

    for bootstrap in batches {
        let batch_mass = state_mass(bootstrap, &state);
        let copper_ppm = state
            .inventory()
            .get_lot(bootstrap.ore_lot)
            .map(|lot| lot.composition().parts_per_million(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("liberation campaign ore lot disappeared"));
        let mut scenario = PrimitiveLiberationScenario {
            charge_policy: support::ChargePolicy::BatchDemand,
            charges: Vec::new(),
            state,
            batch_mass,
            copper_ppm,
            ore: bootstrap.ore,
            crushed: bootstrap.crushed,
            ground: bootstrap.ground,
            undersize: bootstrap.undersize,
            oversize: bootstrap.oversize,
            concentrate: bootstrap.concentrate,
            tailings: bootstrap.tailings,
            fine_tailings: bootstrap.fine_tailings,
            exhausted_tailings: bootstrap.exhausted_tailings,
            native_copper: bootstrap.native_copper,
            ore_lot: bootstrap.ore_lot,
            crusher: infrastructure.crusher,
            quern: infrastructure.quern,
            screen: infrastructure.screen,
            separator: infrastructure.separator,
            power_method: infrastructure.power_method,
            power_provider: infrastructure.power_provider,
            drive: infrastructure.drive,
        };
        let primary = primary::run(registries, &mut scenario);
        let scavenged = scavenging::run(registries, &mut scenario, &primary);
        let cleaned = cleanup::run(registries, &mut scenario);
        recovered_native = recovered_native
            .checked_add(cleaned.native_copper_mass)
            .unwrap_or_else(|| panic!("liberation powered campaign native copper overflowed"));
        batch_charge_ticks.push(
            scenario
                .charges
                .iter()
                .try_fold(0_u64, |total, charge| total.checked_add(charge.ticks))
                .unwrap_or_else(|| panic!("liberation campaign charge attention overflowed")),
        );
        assert!(
            !scavenged.concentrate_mass.is_zero(),
            "liberation campaign batch must retain a cleanup feed"
        );
        state = scenario.state;
    }

    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("liberation campaign matter audit failed: {error}"))
            .total(),
        matter_before,
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("liberation campaign final state invalid: {error}"));
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation campaign player disappeared after execution"));
    let condition = |equipment| {
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition().parts_per_million())
            .unwrap_or_else(|| panic!("liberation campaign equipment disappeared"))
    };
    PrimitiveLiberationCampaignLifecycle {
        batch_charge_ticks,
        elapsed_ticks: state.tick().value() - started_at,
        metabolic_cost_nj: survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy())
            .unwrap_or_else(|| panic!("liberation campaign metabolic reserve increased"))
            .nanojoules(),
        hydration_cost_ul: survival_before
            .hydration()
            .checked_sub(survival_after.hydration())
            .unwrap_or_else(|| panic!("liberation campaign hydration reserve increased"))
            .microliters(),
        recovered_native,
        crusher_condition_ppm: condition(infrastructure.crusher),
        quern_condition_ppm: condition(infrastructure.quern),
        screen_condition_ppm: condition(infrastructure.screen),
        separator_condition_ppm: condition(infrastructure.separator),
        power_provider_condition_ppm: condition(infrastructure.power_provider),
    }
}

fn run_manual_campaign_lifecycle(
    registries: &Registries,
    mut state: AppState,
    batches: &[PrimitiveLiberationBootstrap],
) -> ManualLiberationCampaignLifecycle {
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual liberation campaign matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state).unwrap_or_else(|| {
        panic!("manual liberation campaign player disappeared before execution")
    });
    let started_at = state.tick().value();
    let mut attention_ticks = 0_u64;
    let mut recovered_native = Mass::ZERO;

    for bootstrap in batches {
        let feed_mass = state_mass(bootstrap, &state);
        let execution = execute_manual_ore_recovery(
            registries,
            &mut state,
            ManualOreRecoveryPlan {
                ore_source: bootstrap.ore,
                crushed_destination: bootstrap.manual_crushed,
                native_destination: bootstrap.manual_native,
                residue_destination: bootstrap.manual_residue,
                feed_mass,
            },
        );
        attention_ticks = attention_ticks
            .checked_add(execution.attention_ticks)
            .unwrap_or_else(|| panic!("manual liberation campaign attention overflowed"));
        recovered_native = recovered_native
            .checked_add(execution.recovered_native)
            .unwrap_or_else(|| panic!("manual liberation campaign native copper overflowed"));
    }

    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "manual liberation campaign matter audit failed: {error}"
            ))
            .total(),
        matter_before,
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("manual liberation campaign final state invalid: {error}"));
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("manual liberation campaign player disappeared after execution"));
    let elapsed_ticks = state
        .tick()
        .value()
        .checked_sub(started_at)
        .unwrap_or_else(|| unreachable!("manual liberation campaign cannot run backward"));
    assert_eq!(
        elapsed_ticks, attention_ticks,
        "manual liberation fallback is fully player-attended work"
    );
    ManualLiberationCampaignLifecycle {
        attention_ticks,
        elapsed_ticks,
        metabolic_cost_nj: survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy())
            .unwrap_or_else(|| panic!("manual liberation campaign metabolic reserve increased"))
            .nanojoules(),
        hydration_cost_ul: survival_before
            .hydration()
            .checked_sub(survival_after.hydration())
            .unwrap_or_else(|| panic!("manual liberation campaign hydration reserve increased"))
            .microliters(),
        recovered_native,
    }
}

fn state_mass(bootstrap: &PrimitiveLiberationBootstrap, state: &AppState) -> Mass {
    state
        .inventory()
        .get_lot(bootstrap.ore_lot)
        .map(|lot| lot.mass())
        .unwrap_or_else(|| panic!("liberation campaign ore lot disappeared"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LiberationExtensionChoice {
    ManualFallback,
    BuildKit,
}

impl LiberationExtensionChoice {
    const fn label(self) -> &'static str {
        match self {
            Self::ManualFallback => "manual-fallback",
            Self::BuildKit => "build-kit",
        }
    }
}

#[derive(Clone, Copy)]
struct LiberationExtensionPlan {
    choice: LiberationExtensionChoice,
    manual_campaign_attention: u64,
    acquisition_attention: u64,
    conservative_charge_attention: u64,
    powered_attention_upper: u64,
    minimum_return_ppm: u64,
    minimum_attention_return: u64,
}

fn plan_liberation_extension(
    registries: &Registries,
    case: FocusedProbeCase,
    batch_mass: Mass,
    planned_batches: u64,
) -> LiberationExtensionPlan {
    assert!(planned_batches > 0);
    let manual_per_batch = project_manual_ore_recovery_attention(registries, batch_mass);
    let manual_campaign_attention = manual_per_batch
        .checked_mul(planned_batches)
        .unwrap_or_else(|| panic!("liberation manual campaign projection overflowed"));
    let acquisition = acquisition::project_incremental_kit_acquisition_attention(registries);
    let provider_condition = inherited_progression_condition(
        registries,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        case,
        0x4352_414E_4B00_0001,
    );
    let store = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("liberation inherited flywheel definition disappeared"));
    let stage_count = u64::try_from(LIBERATION_POWERED_ROUTE_STAGES.len())
        .unwrap_or_else(|_| unreachable!("bounded liberation stage count fits u64"));
    let charge_count = stage_count
        .checked_mul(planned_batches)
        .unwrap_or_else(|| panic!("liberation conservative charge count overflowed"));
    let charge_projection = project_manual_power_sequence(
        registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            starting_condition: provider_condition,
            store: ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: store.capacity(),
            charges: charge_count,
        },
        "liberation conservative powered-route charging",
    );
    let powered_attention_upper = acquisition
        .total_ticks
        .checked_add(charge_projection.attention_ticks)
        .unwrap_or_else(|| panic!("liberation powered attention upper bound overflowed"));
    let investment_policy = CapitalInvestmentPolicy::baseline();
    let minimum_return_ppm = investment_policy.minimum_return_ppm();
    let minimum_attention_return =
        investment_policy.minimum_attention_return(0, acquisition.total_ticks);
    let choice = if clears_attention_return(
        manual_campaign_attention,
        powered_attention_upper,
        minimum_attention_return,
    ) {
        LiberationExtensionChoice::BuildKit
    } else {
        LiberationExtensionChoice::ManualFallback
    };
    LiberationExtensionPlan {
        choice,
        manual_campaign_attention,
        acquisition_attention: acquisition.total_ticks,
        conservative_charge_attention: charge_projection.attention_ticks,
        powered_attention_upper,
        minimum_return_ppm,
        minimum_attention_return,
    }
}

fn disclosed_campaign_batches(case: FocusedProbeCase) -> u64 {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor => PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES,
        FocusedProbeRole::MaintainedCoverage
            if case.seed() == LIBERATION_MANUAL_FALLBACK_COVERAGE_SEED =>
        {
            1
        }
        FocusedProbeRole::MaintainedCoverage => PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES,
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            // Campaign horizon is player-visible demand. Generate it independently of the live
            // extension payback threshold so the investment outcome is observed rather than baked
            // into the fixture. Low-bit strata spread small reports across the current campaign
            // envelope while mixed entropy varies the exact horizon.
            let stratum = case.seed() & 0b11;
            let width = PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES.div_ceil(4);
            let lower = 1 + stratum * width;
            let upper = lower
                .saturating_add(width.saturating_sub(1))
                .min(PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES);
            lower + mix64(case.seed() ^ 0x4C49_4245_5248_4F52) % (upper - lower + 1)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrimitiveLiberationWorldParameters {
    planned_batches: u64,
    batch_mass: Mass,
    copper_ppm: u32,
    clay_share_ppm: u32,
}

fn primitive_liberation_batch_ceiling(registries: &Registries, case: FocusedProbeCase) -> Mass {
    [
        (
            PROCESS_CRUSH_ORE,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            inherited_progression_condition(
                registries,
                EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
                case,
                0x4352_5553_4800_0001,
            ),
        ),
        (
            PROCESS_GRIND_CRUSHED_ORE,
            EQUIPMENT_STONE_ROTARY_QUERN,
            Condition::PRISTINE,
        ),
        (
            PROCESS_SCREEN_CRUSHED_ORE,
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            Condition::PRISTINE,
        ),
        (
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            EQUIPMENT_STONE_ROTARY_QUERN,
            Condition::PRISTINE,
        ),
        (
            PROCESS_CONCENTRATE_COPPER,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            inherited_progression_condition(
                registries,
                EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
                case,
                0x5345_5041_5241_544F,
            ),
        ),
    ]
    .into_iter()
    .map(|(process, equipment, condition)| {
        project_powered_ore_replenished_batch_capacity(
            registries,
            process,
            equipment,
            condition,
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        )
        .unwrap_or_else(|error| {
            panic!("primitive liberation authored batch projection failed: {error}")
        })
    })
    .min()
    .unwrap_or_else(|| unreachable!("primitive liberation has a nonempty processing route"))
}

fn primitive_liberation_world_parameters(
    registries: &Registries,
    case: FocusedProbeCase,
) -> PrimitiveLiberationWorldParameters {
    let seed = case.seed();
    let route_ceiling = primitive_liberation_batch_ceiling(registries, case).milligrams();
    // Exercise meaningful utilization of the complete inherited route without pinning a copied
    // authored mass. Workload horizon, rather than tiny feed parcels, owns the build/manual capital
    // crossover; exact batch mass still moves materially across the current production bottleneck.
    let minimum_batch = (route_ceiling / 2).max(1);
    let maximum_batch = route_ceiling
        .checked_mul(4)
        .unwrap_or_else(|| panic!("primitive liberation route ceiling scaling overflowed"))
        / 5;
    let maximum_batch = maximum_batch.max(minimum_batch);
    let requested_batch_mass = Mass::from_milligrams(
        minimum_batch + mix64(seed ^ 0x4C49_4245_5241_5445) % (maximum_batch - minimum_batch + 1),
    );
    let grinding = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation grinding definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation screening definition disappeared"));
    let batch_mass = resolve_representable_screening_mass(
        screening,
        grinding.output_particle_size_distribution(),
        requested_batch_mass,
    )
    .unwrap_or_else(|error| panic!("primitive liberation batch planning failed: {error}"));
    assert!(
        !batch_mass.is_zero(),
        "primitive liberation generated no representable batch"
    );
    let planned_batches = disclosed_campaign_batches(case);
    PrimitiveLiberationWorldParameters {
        planned_batches,
        batch_mass,
        copper_ppm: 300_000 + (mix64(seed ^ 0x4C49_4245_5243_5550) % 300_001) as u32,
        clay_share_ppm: (mix64(seed ^ 0x4C49_4245_5243_4C41) % 650_001) as u32,
    }
}

pub(super) fn run_primitive_liberation_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let PrimitiveLiberationWorldParameters {
        planned_batches,
        batch_mass,
        copper_ppm,
        clay_share_ppm,
    } = primitive_liberation_world_parameters(registries, case);
    let extension_plan = plan_liberation_extension(registries, case, batch_mass, planned_batches);
    let selected_build = extension_plan.choice == LiberationExtensionChoice::BuildKit;
    reviewln!(
        "LIBERATION INVESTMENT seed=0x{seed:016X} sample={} disclosed=[batches:{} batch:{}mg] projected=[manual:{}t extension-acquisition:{}t conservative-charging:{}t powered-upper:{}t minimum-return:{}ppm/{}t] selected={} choice-frozen-before-action=true basis=canonical-manual-recovery+conservative-full-stage-charging",
        case.role().label(),
        planned_batches,
        batch_mass.milligrams(),
        extension_plan.manual_campaign_attention,
        extension_plan.acquisition_attention,
        extension_plan.conservative_charge_attention,
        extension_plan.powered_attention_upper,
        extension_plan.minimum_return_ppm,
        extension_plan.minimum_attention_return,
        extension_plan.choice.label(),
    );
    let (acquired, campaign_bootstraps) =
        acquisition::acquire_raw_kit(registries, case, planned_batches, selected_build, |state| {
            (0..planned_batches)
                .map(|_| {
                    bootstrap_liberation_inventory(
                        registries,
                        state,
                        batch_mass,
                        copper_ppm,
                        clay_share_ppm,
                    )
                })
                .collect::<Vec<_>>()
        });
    let manual_campaign = run_manual_campaign_lifecycle(
        registries,
        acquired.decision_state.clone(),
        &campaign_bootstraps,
    );
    assert_eq!(
        manual_campaign.attention_ticks, extension_plan.manual_campaign_attention,
        "liberation manual campaign execution diverged from the pre-action projection"
    );
    let bootstrap = *campaign_bootstraps
        .first()
        .unwrap_or_else(|| panic!("liberation campaign lost its first batch"));
    let campaign_lifecycle = run_powered_campaign_lifecycle(
        registries,
        acquired.state.clone(),
        &campaign_bootstraps,
        PrimitiveLiberationCampaignInfrastructure {
            crusher: acquired.crusher,
            quern: acquired.quern,
            screen: acquired.screen,
            separator: acquired.separator,
            power_method: MANUAL_POWER_HAND_CRANK,
            power_provider: acquired.power_provider,
            drive: acquired.drive,
        },
    );
    let powered_campaign_attention = acquired
        .review
        .attention_ticks
        .checked_add(
            campaign_lifecycle
                .batch_charge_ticks
                .iter()
                .try_fold(0_u64, |total, ticks| total.checked_add(*ticks))
                .unwrap_or_else(|| panic!("liberation campaign charge attention overflowed")),
        )
        .unwrap_or_else(|| panic!("liberation powered campaign attention overflowed"));
    assert!(
        powered_campaign_attention <= extension_plan.powered_attention_upper,
        "liberation conservative powered projection must remain an upper bound on executed active attention"
    );
    let powered_campaign_body_nj = acquired
        .review
        .metabolic_cost_nj
        .checked_add(campaign_lifecycle.metabolic_cost_nj)
        .unwrap_or_else(|| panic!("liberation powered campaign metabolic cost overflowed"));
    let powered_campaign_hydration_ul = acquired
        .review
        .hydration_cost_ul
        .checked_add(campaign_lifecycle.hydration_cost_ul)
        .unwrap_or_else(|| panic!("liberation powered campaign hydration cost overflowed"));
    let powered_campaign_elapsed = acquired
        .review
        .attention_ticks
        .checked_add(campaign_lifecycle.elapsed_ticks)
        .unwrap_or_else(|| panic!("liberation powered campaign elapsed time overflowed"));
    let (
        selected_attention,
        selected_elapsed,
        selected_native,
        selected_metabolic_nj,
        selected_hydration_ul,
    ) = match extension_plan.choice {
        LiberationExtensionChoice::ManualFallback => (
            manual_campaign.attention_ticks,
            manual_campaign.elapsed_ticks,
            manual_campaign.recovered_native,
            manual_campaign.metabolic_cost_nj,
            manual_campaign.hydration_cost_ul,
        ),
        LiberationExtensionChoice::BuildKit => (
            powered_campaign_attention,
            powered_campaign_elapsed,
            campaign_lifecycle.recovered_native,
            powered_campaign_body_nj,
            powered_campaign_hydration_ul,
        ),
    };
    reviewln!(
        "LIBERATION EXPERIENCE seed=0x{seed:016X} sample={} disclosed=[batches:{} batch:{}mg] selected={} extension-built={} execution=[attention:{}t elapsed:{}t native-copper:{}mg body:{}nJ/{}uL] counterfactual=[manual-attention:{}t manual-native:{}mg powered-attention:{}t powered-native:{}mg] choice-frozen-before-action=true matter=conserved",
        case.role().label(),
        planned_batches,
        batch_mass.milligrams(),
        extension_plan.choice.label(),
        selected_build,
        selected_attention,
        selected_elapsed,
        selected_native.milligrams(),
        selected_metabolic_nj,
        selected_hydration_ul,
        manual_campaign.attention_ticks,
        manual_campaign.recovered_native.milligrams(),
        powered_campaign_attention,
        campaign_lifecycle.recovered_native.milligrams(),
    );
    let state = acquired.state;
    let crusher = acquired.crusher;
    let quern = acquired.quern;
    let screen = acquired.screen;
    let separator = acquired.separator;
    let power_provider = acquired.power_provider;
    let drive = acquired.drive;
    let kit_acquisition = acquired.review;
    let PrimitiveLiberationBootstrap {
        ore,
        crushed,
        ground,
        undersize,
        oversize,
        concentrate,
        tailings,
        fine_tailings,
        exhausted_tailings,
        native_copper,
        manual_crushed,
        manual_native,
        manual_residue,
        ore_lot,
    } = bootstrap;
    let drive_definition = state
        .energy()
        .get_store(drive)
        .map(|record| record.definition())
        .unwrap_or_else(|| panic!("inherited primitive liberation drive disappeared"));
    let drive_capacity = registries
        .energy()
        .get_store(drive_definition)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("inherited primitive drive definition disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("primitive liberation matter setup failed: {error}"))
        .total();
    let manual_recovery = evaluate_manual_ore_recovery(
        registries,
        &state,
        ManualOreRecoveryPlan {
            ore_source: ore,
            crushed_destination: manual_crushed,
            native_destination: manual_native,
            residue_destination: manual_residue,
            feed_mass: batch_mass,
        },
    );
    assert_eq!(
        manual_recovery.feed_mass, batch_mass,
        "primitive liberation manual comparison must use the exact powered feed mass"
    );

    let mut scenario = PrimitiveLiberationScenario {
        charge_policy: support::ChargePolicy::BatchDemand,
        charges: Vec::new(),
        state,
        batch_mass,
        copper_ppm,
        ore,
        crushed,
        ground,
        undersize,
        oversize,
        concentrate,
        tailings,
        fine_tailings,
        exhausted_tailings,
        native_copper,
        ore_lot,
        crusher,
        quern,
        screen,
        separator,
        power_method: MANUAL_POWER_HAND_CRANK,
        power_provider,
        drive,
    };
    let mut full_buffer = scenario.clone();
    full_buffer.charge_policy = support::ChargePolicy::FullBuffer;
    let started_at = scenario.state.tick().value();
    let primary = primary::run(registries, &mut scenario);
    let primary_completed_at = scenario.state.tick().value();
    let mut direct_cleanup = scenario.clone();
    let direct_cleaned = cleanup::run(registries, &mut direct_cleanup);
    let scavenged = scavenging::run(registries, &mut scenario, &primary);
    let scavenger_completed_at = scenario.state.tick().value();
    let cleaned = cleanup::run(registries, &mut scenario);
    let baseline_primary = primary::run(registries, &mut full_buffer);
    let baseline_scavenged = scavenging::run(registries, &mut full_buffer, &baseline_primary);
    let baseline_cleaned = cleanup::run(registries, &mut full_buffer);
    assert_eq!(
        primary, baseline_primary,
        "charging policy must preserve primary recovery"
    );
    assert_eq!(
        scavenged, baseline_scavenged,
        "charging policy must preserve scavenger recovery"
    );
    assert_eq!(
        cleaned, baseline_cleaned,
        "charging policy must preserve final concentrate cleanup"
    );
    comparison::review(
        registries,
        case,
        comparison::LiberationComparison {
            started_at,
            primary_completed_at,
            scavenger_completed_at,
            demand: &scenario,
            full: &full_buffer,
            scavenged: &scavenged,
            cleaned: &cleaned,
            direct_cleanup: &direct_cleanup,
            direct_cleaned: &direct_cleaned,
            manual_recovery: &manual_recovery,
            kit_acquisition: &kit_acquisition,
            campaign_lifecycle: &campaign_lifecycle,
            planned_batches,
            extension_selected: selected_build,
        },
    );
    let state = &scenario.state;
    assert!(
        state
            .energy()
            .get_store(drive)
            .is_some_and(|record| record.stored() < drive_capacity)
    );
    for equipment in [crusher, quern, screen, separator] {
        assert!(
            state
                .equipment()
                .get_equipment(equipment)
                .is_some_and(|record| record.condition() < Condition::PRISTINE),
            "every primitive liberation machine must incur real operation wear"
        );
    }
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!("primitive liberation matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("primitive liberation trusted-state audit failed: {error}"));
    let final_energy = state
        .energy()
        .get_store(drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("primitive liberation drive disappeared after completion"));
    let recovered_copper_ppm_mg = primary
        .concentrate_copper_ppm_mg
        .checked_add(scavenged.additional_recovered_copper_ppm_mg)
        .unwrap_or_else(|| panic!("primitive liberation recovered-copper audit overflowed"));
    let native_copper_mass = state
        .inventory()
        .get_stockpile(native_copper)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive native-copper stockpile disappeared"));
    assert_eq!(native_copper_mass, cleaned.native_copper_mass);
    // Use exact constituent numerators, not rounded concentrate grades. Keep any fractional
    // milligrams as decimal digits without floating point or truncating represented copper.
    let copper_mg = |numerator: u128| {
        let whole = numerator / 1_000_000;
        let fraction = numerator % 1_000_000;
        if fraction == 0 {
            whole.to_string()
        } else {
            let digits = format!("{fraction:06}");
            format!("{whole}.{}", digits.trim_end_matches('0'))
        }
    };
    let furnace = registries
        .equipment()
        .get_equipment(EQUIPMENT_ELECTRIC_FURNACE)
        .unwrap_or_else(|| panic!("foundry frontier furnace definition disappeared"));
    let mold = registries
        .equipment()
        .get_equipment(EQUIPMENT_CASTING_MOLD)
        .unwrap_or_else(|| panic!("foundry frontier mold definition disappeared"));
    let electrical_buffer = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("foundry frontier electrical buffer definition disappeared"));
    let thermal_sink = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .unwrap_or_else(|| panic!("foundry frontier thermal sink definition disappeared"));
    let ordinary_manual_electrical = ordinary_manual_electrical_power_providers(registries);
    let manual_electrical_generation = !ordinary_manual_electrical.is_empty();
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("foundry frontier copper melting definition disappeared"));
    let furnace_heating_power = match furnace
        .capabilities()
        .get_capability(melting.heating_power_capability())
    {
        Some(CapabilityValue::Power(power)) => power,
        Some(other) => panic!(
            "foundry furnace heating capability changed physical kind: {:?}",
            other.kind()
        ),
        None => panic!("foundry furnace lost its authored heating capability"),
    };
    let maximum_manual_electrical_power = ordinary_manual_electrical
        .iter()
        .map(|(_, _, power)| *power)
        .max()
        .unwrap_or_else(|| {
            panic!("ordinary manual electrical power has no acquirable authored provider")
        });
    let manual_microwatts = maximum_manual_electrical_power
        .whole_microwatts()
        .unwrap_or_else(|| panic!("manual electrical frontier power is not a whole microwatt"));
    let furnace_microwatts = furnace_heating_power
        .whole_microwatts()
        .unwrap_or_else(|| panic!("furnace heating frontier power is not a whole microwatt"));
    let transfer_ceiling_ratio = furnace_heating_power
        .picowatts()
        .checked_div(maximum_manual_electrical_power.picowatts())
        .unwrap_or_else(|| panic!("manual electrical frontier power unexpectedly vanished"));
    let foundry_frontier = format!(
        "assembly-edge=[furnace:{} mold:{} electrical-buffer:{} thermal-sink:{}] manual-electrical-generation:{} manual-electrical-reachability=authored-acquisition-edge support-required=[furnace:{} mold:{}] energy-scale=[manual-electrical-max:{}uW industrial-furnace-transfer-ceiling:{}uW ceiling-ratio:{}x melting-carrier:{:?} conversion-path:present]",
        furnace.assembly_profile().is_some(),
        mold.assembly_profile().is_some(),
        electrical_buffer.assembly_profile().is_some(),
        thermal_sink.assembly_profile().is_some(),
        manual_electrical_generation,
        furnace.requires_structural_support(),
        mold.requires_structural_support(),
        manual_microwatts,
        furnace_microwatts,
        transfer_ceiling_ratio,
        melting.energy_carrier(),
    );
    reviewln!(
        "LIBERATION FRONTIER CAPABILITY seed=0x{seed:016X} sample={} cleanup-executed=true reason=required-native-copper-conversion route=reinforced-hand-crank+copper-banded-flywheel->reinforced-crusher->quern->timber-riddle->regrind->reinforced-separator->tailings-regrind->scavenger->concentrate-cleanup input=[{}mg {}ppm-Cu clay-share:{}ppm] concentrate=[first:{}mg/{}ppm final:{}mg/{}ppm] copper-in-concentrate=[first:{}mg final:{}mg scavenger-recovered:{}mg] native-copper={}mg cleanup-residue={}mg exhausted-tailings={}mg stored-work-remaining={}nJ machinery-worn=true matter=conserved",
        case.role().label(),
        batch_mass.milligrams(),
        copper_ppm,
        clay_share_ppm,
        primary.concentrate_mass.milligrams(),
        primary.concentrate_grade_ppm,
        scavenged.concentrate_mass.milligrams(),
        scavenged.concentrate_grade_ppm,
        copper_mg(primary.concentrate_copper_ppm_mg),
        copper_mg(recovered_copper_ppm_mg),
        copper_mg(scavenged.additional_recovered_copper_ppm_mg),
        cleaned.native_copper_mass.milligrams(),
        cleaned.cleanup_residue_mass.milligrams(),
        cleaned.exhausted_tailings_mass.milligrams(),
        final_energy.nanojoules(),
    );
    let scavenger_share_ppm = scavenged
        .additional_recovered_copper_ppm_mg
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("primitive liberation scavenger-share audit overflowed"))
        .checked_div(recovered_copper_ppm_mg)
        .unwrap_or(0)
        .min(1_000_000);
    reviewln!(
        "LIBERATION FRONTIER seed=0x{seed:016X} sample={} input=[{}mg {}ppm-Cu] concentrate=[final:{}mg/{}ppm] scavenger=[extra-copper:{}mg share:{}ppm-of-recovered-copper] cleanup=[native-copper:{}mg recovery:{}ppm residue:{}mg] sink=usable-native-copper remaining-frontier=industrial-foundry-scale ordinary-continuation=[first-foundry->80g-batch-upgrades] industrial-foundry-frontier=[{}] reachability-authority=STATUS.md",
        case.role().label(),
        batch_mass.milligrams(),
        copper_ppm,
        scavenged.concentrate_mass.milligrams(),
        scavenged.concentrate_grade_ppm,
        scavenged.additional_recovered_copper_ppm_mg / 1_000_000,
        scavenger_share_ppm,
        cleaned.native_copper_mass.milligrams(),
        cleaned.target_recovery_ppm,
        cleaned.cleanup_residue_mass.milligrams(),
        foundry_frontier,
    );
}
