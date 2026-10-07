//! Matched primitive and settlement human-power comparisons through canonical craft and charging.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{seed_composed_lot, seed_lot};
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_TIMBER_SASH_SAWMILL,
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WOOD, PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;

use super::bulk_fieldwork_workload::{
    BULK_FIELDWORK_ORDER_MAX_BATCHES, BULK_FIELDWORK_ORDER_MIN_BATCHES, bulk_fieldwork_order_mass,
    primitive_quarry_batch_mass,
};
use super::capital_investment_policy::CapitalInvestmentPolicy;
#[cfg(not(test))]
use super::capital_investment_policy::clears_attention_return;
use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::nominal_equipment_mass_capability;
use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::inventory_support::add_solid_stockpile;
use super::manual_construction_planning::manual_construction_route_from_roots;
use super::ore_fixture::copper_ore_composition;
#[cfg(not(test))]
use super::physical_time::format_physical_duration;
use super::primitive_workload::{STOCKPILE_WORK_ORDER_CYCLES, primitive_mining_cycle_mass};
use super::seed::mix64;
use super::settlement_demand::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, organic_lumber_batches,
};
use super::stationary_survival::{
    StationarySurvivalStart, admit_stationary_player_with_survival_start,
};

#[path = "power_provider_build.rs"]
mod build;
#[path = "power_provider_consumers.rs"]
mod consumers;
#[path = "power_provider_execution.rs"]
mod execution;
#[path = "power_provider_planning.rs"]
mod planning;
#[path = "power_provider_provisioning.rs"]
mod provisioning;

use consumers::{build_primitive_power_consumer, build_settlement_power_consumer};
use execution::{
    PrimitiveComparison, ProjectExecutionResources, SettlementComparison,
    execute_primitive_comparison, execute_selected_primitive_project,
    execute_selected_settlement_project, execute_settlement_comparison,
};
#[cfg(not(test))]
use planning::{PowerProviderChoice, PrimitivePowerChoice, SettlementPowerChoice};
use planning::{
    PrimitivePowerProject, SettlementCopperPolicy, primitive_power_plan,
    reachable_mechanical_power_providers, settlement_power_plan,
};
use provisioning::seed_power_project_provisions;

fn investment_policy(case: FocusedProbeCase) -> CapitalInvestmentPolicy {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage => {
            CapitalInvestmentPolicy::baseline()
        }
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            CapitalInvestmentPolicy::from_behavior_seed(
                case.required_behavior_seed("power-provider investment policy"),
            )
        }
    }
}

#[derive(Clone, Copy)]
enum PowerProjectEra {
    Primitive,
    Settlement,
}

fn power_project_survival_start(
    case: FocusedProbeCase,
    era: PowerProjectEra,
) -> StationarySurvivalStart {
    if matches!(
        case.role(),
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage
    ) {
        return StationarySurvivalStart::FullReserve;
    }

    // The four-case exploratory sample already stratifies these low world bits. Pair each ordinary
    // workload scale with one rested and one pressured start so survival is a recurring part of
    // power planning instead of a maintained-only edge case. Settlement rotates the same disclosed
    // physical stratum so its pressure is not mechanically identical to the primitive subepisode.
    let stratum = usize::try_from(case.seed() & 0b11)
        .unwrap_or_else(|_| unreachable!("two-bit power stratum fits usize"));
    let primitive = [
        StationarySurvivalStart::FullReserve,
        StationarySurvivalStart::HungerWarningBoundary,
        StationarySurvivalStart::FullReserve,
        StationarySurvivalStart::HydrationWarningBoundary,
    ];
    let settlement = [
        StationarySurvivalStart::HydrationWarningBoundary,
        StationarySurvivalStart::FullReserve,
        StationarySurvivalStart::HungerWarningBoundary,
        StationarySurvivalStart::FullReserve,
    ];
    match era {
        PowerProjectEra::Primitive => primitive[stratum],
        PowerProjectEra::Settlement => settlement[stratum],
    }
}

#[cfg(not(test))]
fn primitive_frontier_label(frontier: &[(u64, PowerProviderChoice)]) -> String {
    frontier
        .iter()
        .map(|(charges, choice)| format!("{charges}:{}", choice.primitive_label()))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(not(test))]
fn settlement_frontier_label(frontier: &[(u64, PowerProviderChoice)]) -> String {
    frontier
        .iter()
        .map(|(charges, choice)| format!("{charges}:{}", choice.settlement_label()))
        .collect::<Vec<_>>()
        .join(",")
}

fn settlement_copper_policy(case: FocusedProbeCase) -> SettlementCopperPolicy {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor => SettlementCopperPolicy::SpendAvailable,
        FocusedProbeRole::MaintainedCoverage => SettlementCopperPolicy::PreserveForOtherUses,
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            let behavior = case.required_behavior_seed("settlement power copper policy");
            if mix64(behavior ^ 0x5345_5454_5F43_4F50).is_multiple_of(2) {
                SettlementCopperPolicy::SpendAvailable
            } else {
                SettlementCopperPolicy::PreserveForOtherUses
            }
        }
    }
}

fn add_raw_requirement(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) {
    requirements
        .entry(commodity)
        .and_modify(|existing| {
            *existing = existing
                .checked_add(mass)
                .unwrap_or_else(|| panic!("{context} raw requirement overflowed"));
        })
        .or_insert(mass);
}

fn power_raw_roots(
    state: &AppState,
    raw: deep_hearth::inventory::StockpileId,
) -> Vec<CommodityKey> {
    let stockpile = state
        .inventory()
        .get_stockpile(raw)
        .unwrap_or_else(|| panic!("power-provider raw stockpile disappeared"));
    [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ]
    .into_iter()
    .filter(|commodity| !stockpile.get_mass(*commodity).is_zero())
    .collect()
}

fn add_output_as_raw_requirement(
    registries: &Registries,
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    primitive_roots: &[CommodityKey],
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
) {
    let route = manual_construction_route_from_roots(
        registries,
        commodity,
        required,
        primitive_roots,
        context,
    )
    .unwrap_or_else(|| {
        panic!(
            "{context} has no equipment-free manual route from disclosed raw roots to commodity {}",
            commodity.value()
        )
    });
    add_raw_requirement(requirements, route.raw_commodity, route.raw_mass, context);
}

fn add_equipment_raw_requirements(
    registries: &Registries,
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    primitive_roots: &[CommodityKey],
    definition: EquipmentDefinitionId,
    context: &'static str,
) {
    let profile = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|equipment| equipment.assembly_profile())
        .unwrap_or_else(|| panic!("{context} equipment lost authored assembly"));
    for input in profile.inputs() {
        add_output_as_raw_requirement(
            registries,
            requirements,
            primitive_roots,
            input.commodity(),
            input.mass(),
            context,
        );
    }
}

fn add_store_raw_requirements(
    registries: &Registries,
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    primitive_roots: &[CommodityKey],
    definition: EnergyStoreDefinitionId,
    context: &'static str,
) {
    let profile = registries
        .energy()
        .get_store(definition)
        .and_then(|store| store.assembly_profile())
        .unwrap_or_else(|| panic!("{context} energy store lost authored assembly"));
    for input in profile.inputs() {
        add_output_as_raw_requirement(
            registries,
            requirements,
            primitive_roots,
            input.commodity(),
            input.mass(),
            context,
        );
    }
}

fn add_consumer_service_reserve(
    registries: &Registries,
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    primitive_roots: &[CommodityKey],
    consumer: EquipmentDefinitionId,
    maximum_services: u64,
    context: &'static str,
) {
    let maintenance = registries
        .equipment()
        .get_equipment(consumer)
        .and_then(|equipment| equipment.maintenance_profile())
        .unwrap_or_else(|| panic!("{context} consumer lost authored maintenance"));
    let total_replacement = Mass::from_milligrams(
        maintenance
            .full_service_replacement_mass()
            .milligrams()
            .checked_mul(maximum_services)
            .unwrap_or_else(|| panic!("{context} service reserve overflowed")),
    );
    add_output_as_raw_requirement(
        registries,
        requirements,
        primitive_roots,
        maintenance.replacement(),
        total_replacement,
        context,
    );
}

fn power_raw_opportunity(
    registries: &Registries,
    primitive_roots: &[CommodityKey],
    equipment: &[EquipmentDefinitionId],
    stores: &[EnergyStoreDefinitionId],
    serviced_consumer: EquipmentDefinitionId,
    maximum_services: u64,
    context: &'static str,
) -> BTreeMap<CommodityKey, Mass> {
    let mut requirements = BTreeMap::new();
    for &definition in equipment {
        add_equipment_raw_requirements(
            registries,
            &mut requirements,
            primitive_roots,
            definition,
            context,
        );
    }
    for &definition in stores {
        add_store_raw_requirements(
            registries,
            &mut requirements,
            primitive_roots,
            definition,
            context,
        );
    }
    add_consumer_service_reserve(
        registries,
        &mut requirements,
        primitive_roots,
        serviced_consumer,
        maximum_services,
        context,
    );
    requirements
}

fn power_provider_equipment_for_roots(
    registries: &Registries,
    roots: &[CommodityKey],
) -> Vec<EquipmentDefinitionId> {
    reachable_mechanical_power_providers(registries, roots.iter().copied())
        .into_iter()
        .map(|choice| choice.equipment)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn seed_raw_opportunity(
    registries: &Registries,
    state: &mut AppState,
    requirements: BTreeMap<CommodityKey, Mass>,
    context: &'static str,
) -> (deep_hearth::inventory::StockpileId, Mass) {
    let capacity = requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("{context} total raw opportunity overflowed"));
    let raw = add_solid_stockpile(state, capacity);
    for (commodity, mass) in requirements {
        seed_lot(registries, state, raw, commodity, mass, ROOM_TEMPERATURE);
    }
    (raw, capacity)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrimitiveCrushingWorkload {
    RoutineStockpile,
    BulkFieldwork,
}

const ROUTINE_STOCKPILE_MIN_CYCLES: u64 = STOCKPILE_WORK_ORDER_CYCLES * 2 / 3;
const ROUTINE_STOCKPILE_MAX_CYCLES: u64 = STOCKPILE_WORK_ORDER_CYCLES * 2;

impl PrimitiveCrushingWorkload {
    #[cfg(not(test))]
    const fn label(self) -> &'static str {
        match self {
            Self::RoutineStockpile => "routine-stockpile",
            Self::BulkFieldwork => "bulk-fieldwork-ore",
        }
    }
}

fn declared_primitive_crushing_project(
    registries: &Registries,
    seed: u64,
    store_definition: EnergyStoreDefinitionId,
) -> (Mass, Energy, PrimitiveCrushingWorkload) {
    // Early powered processing sees two ordinary disclosed workload scales. Routine stockpiling
    // follows the progression loop; bulk work carries forward the same large extraction order used
    // by fieldwork. This lets better power equipment emerge from a real upstream opportunity rather
    // than from a synthetic provider-crossover benchmark.
    let definition = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive power project crusher process disappeared"));
    let store = registries
        .energy()
        .get_store(store_definition)
        .unwrap_or_else(|| panic!("primitive power project accumulator disappeared"));
    let cycle_mass = primitive_mining_cycle_mass(registries, seed);
    let cycle_work = deep_hearth::energy::calculate_mass_specific_energy(
        cycle_mass,
        definition.specific_energy(),
    );
    assert!(
        !cycle_work.is_zero() && cycle_work <= store.capacity(),
        "ordinary primitive mining cycle must fit the current baseline accumulator"
    );
    // Low world-seed bits are a sampling stratum, not actor policy. They keep a tiny bounded report
    // spread across ordinary upstream workload scales while mixed high entropy still varies the
    // exact demand inside each scale.
    let stratum = seed & 0b11;
    let workload = if stratum >= 2 {
        PrimitiveCrushingWorkload::BulkFieldwork
    } else {
        PrimitiveCrushingWorkload::RoutineStockpile
    };
    let mass = match workload {
        PrimitiveCrushingWorkload::RoutineStockpile => {
            let midpoint = (ROUTINE_STOCKPILE_MIN_CYCLES + ROUTINE_STOCKPILE_MAX_CYCLES) / 2;
            let (minimum, maximum) = if stratum == 0 {
                (ROUTINE_STOCKPILE_MIN_CYCLES, midpoint)
            } else {
                (midpoint + 1, ROUTINE_STOCKPILE_MAX_CYCLES)
            };
            let cycles = minimum + mix64(seed ^ 0x5052_494D_5F4F_5245) % (maximum - minimum + 1);
            Mass::from_milligrams(
                cycle_mass
                    .milligrams()
                    .checked_mul(cycles)
                    .unwrap_or_else(|| panic!("primitive power project mass overflowed")),
            )
        }
        PrimitiveCrushingWorkload::BulkFieldwork => {
            let quarry_batch = primitive_quarry_batch_mass(registries);
            let ordinary = bulk_fieldwork_order_mass(registries, seed);
            let ordinary_batches = ordinary.milligrams() / quarry_batch.milligrams();
            let ordinal = ordinary_batches - BULK_FIELDWORK_ORDER_MIN_BATCHES;
            // Keep the two bulk strata on opposite halves of the ordinary fieldwork envelope, but
            // never search for a workload that forces a provider identity. The chosen provider is
            // an observed result of live economics and actor policy.
            let midpoint =
                (BULK_FIELDWORK_ORDER_MIN_BATCHES + BULK_FIELDWORK_ORDER_MAX_BATCHES) / 2;
            let (minimum, maximum) = if stratum == 2 {
                (BULK_FIELDWORK_ORDER_MIN_BATCHES, midpoint)
            } else {
                (midpoint + 1, BULK_FIELDWORK_ORDER_MAX_BATCHES)
            };
            let batches = minimum + ordinal % (maximum - minimum + 1);
            Mass::from_milligrams(
                quarry_batch
                    .milligrams()
                    .checked_mul(batches)
                    .unwrap_or_else(|| panic!("primitive power bulk project mass overflowed")),
            )
        }
    };
    (
        mass,
        deep_hearth::energy::calculate_mass_specific_energy(mass, definition.specific_energy()),
        workload,
    )
}

pub(super) fn declared_settlement_lumber_project(
    registries: &Registries,
    seed: u64,
) -> (Mass, Energy) {
    // Reuse the same disclosed lumber-demand generator as lived settlement play. Power-provider
    // evaluation observes how the current provider market serves that upstream demand; it must not
    // define the physical workload from its own crossover-search horizon.
    let definition = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement power project saw process disappeared"));
    let batch_mass = registries
        .crafting()
        .get_manual(definition.transform())
        .map(|transform| transform.input_mass())
        .unwrap_or_else(|| panic!("settlement power project manual saw transform disappeared"));
    let batches = organic_lumber_batches(
        seed & 0b11,
        mix64(seed ^ 0x5345_5454_4C55_4D42),
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
    );
    let mass = Mass::from_milligrams(
        batch_mass
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("settlement power project mass overflowed")),
    );
    (
        mass,
        deep_hearth::energy::calculate_mass_specific_energy(mass, definition.specific_energy()),
    )
}

#[cfg(test)]
include_power_provider_generation_contract_tests!();

fn primitive_accumulator_for_current_crusher(registries: &Registries) -> EnergyStoreDefinitionId {
    let process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive power project crusher process disappeared"));
    let maximum_batch = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_CRUSHER,
        process.max_batch_mass_capability(),
    );
    let required = deep_hearth::energy::calculate_mass_specific_energy(
        maximum_batch,
        process.specific_energy(),
    );

    let candidates = registries
        .energy()
        .definitions()
        .filter(|definition| {
            definition.carrier() == process.energy_carrier()
                && definition.capacity() >= required
                && !definition.max_input_power().is_zero()
                && !definition.max_output_power().is_zero()
                && definition.assembly_profile().is_some_and(|assembly| {
                    assembly.inputs().iter().all(|input| {
                        matches!(input.commodity().material(), MATERIAL_STONE | MATERIAL_WOOD)
                    })
                })
        })
        .map(|definition| {
            let key = (
                definition.capacity().nanojoules(),
                definition
                    .assembly_profile()
                    .map(|assembly| assembly.input_mass().milligrams())
                    .unwrap_or(u64::MAX),
            );
            (definition.id(), key)
        })
        .collect::<Vec<_>>();
    let best_key = candidates
        .iter()
        .map(|(_, key)| *key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "no ordinary copper-free mechanical accumulator can fund one pristine crusher batch of {}mg requiring {}nJ",
                maximum_batch.milligrams(),
                required.nanojoules(),
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, key)| *key == best_key)
        .map(|(definition, _)| definition);
    let selected = best
        .next()
        .unwrap_or_else(|| unreachable!("best accumulator key came from one candidate"));
    assert!(
        best.next().is_none(),
        "ordinary copper-free accumulators are physically tied at the actor's minimum capacity/material key; author an observable preference instead of using definition identity"
    );
    selected
}

pub(super) fn run_power_provider_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let investment_policy = investment_policy(case);
    let primitive_survival_start = power_project_survival_start(case, PowerProjectEra::Primitive);
    let settlement_survival_start = power_project_survival_start(case, PowerProjectEra::Settlement);
    // Derive the smallest ordinary copper-free accumulator that funds one complete pristine
    // crusher batch from the current content graph. This keeps the player policy stable when
    // crusher energy, batch capacity, or authored storage definitions are retuned.
    let store_definition = primitive_accumulator_for_current_crusher(registries);
    let mut state = AppState::new();
    // Disclosed starting raw matter only: derive the finite opportunity from every candidate build
    // plus a conservative service reserve. Shaped components are still earned through canonical
    // manual production after admission; this raw package is plumbing, not a world-source claim.
    let primitive_roots = [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
    ];
    let mut primitive_provider_equipment =
        power_provider_equipment_for_roots(registries, &primitive_roots);
    primitive_provider_equipment.push(EQUIPMENT_STONE_CRUSHER);
    let primitive_requirements = power_raw_opportunity(
        registries,
        &primitive_roots,
        &primitive_provider_equipment,
        &[store_definition],
        EQUIPMENT_STONE_CRUSHER,
        planning::primitive_project_batch_limit(),
        "primitive power opportunity",
    );
    let (raw, primitive_raw_capacity) = seed_raw_opportunity(
        registries,
        &mut state,
        primitive_requirements,
        "primitive power opportunity",
    );
    let shaped = add_solid_stockpile(&mut state, primitive_raw_capacity);
    let capacity_nj = registries
        .energy()
        .get_store(store_definition)
        .map(|definition| definition.capacity().nanojoules())
        .unwrap_or_else(|| panic!("power provider flywheel definition disappeared"));
    let (primitive_project_mass, primitive_project_work, _primitive_workload) =
        declared_primitive_crushing_project(registries, seed, store_definition);
    let primitive_available_mass = primitive_project_mass;
    let primitive_feed = add_solid_stockpile(&mut state, primitive_available_mass);
    let primitive_output = add_solid_stockpile(&mut state, primitive_available_mass);
    // Service and shaping buffers are not the pressure under test. Bound them by the finite raw
    // opportunity so authored maintenance/component changes cannot create an unrelated fixture cap.
    let primitive_service_replacement = add_solid_stockpile(&mut state, primitive_raw_capacity);
    let primitive_service_spent = add_solid_stockpile(&mut state, primitive_raw_capacity);
    let primitive_provisions = seed_power_project_provisions(registries, &mut state);
    seed_composed_lot(
        registries,
        &mut state,
        primitive_feed,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        primitive_available_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(350_000, 200_000),
    );
    admit_stationary_player_with_survival_start(
        registries,
        &mut state,
        &[
            raw,
            primitive_feed,
            primitive_output,
            primitive_service_replacement,
            primitive_service_spent,
            primitive_provisions.food,
            primitive_provisions.enclosure_material,
            shaped,
        ],
        &[primitive_provisions.water],
        primitive_survival_start,
        "primitive power-provider",
    );
    let primitive_consumer = build_primitive_power_consumer(
        registries,
        &mut state,
        raw,
        shaped,
        primitive_feed,
        primitive_output,
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("power provider initial matter audit failed: {error}"))
        .total();
    let fluid_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("power provider initial fluid audit failed: {error}"))
        .total();
    // Freeze the actor's investment choice from current authored topology and canonical physical
    // projections before any matched branch is executed.
    let plan = primitive_power_plan(
        registries,
        &state,
        raw,
        shaped,
        PrimitivePowerProject {
            store_definition,
            capacity_nj,
            consumer: primitive_consumer.equipment(),
            declared_mass: primitive_project_mass,
            declared_work_nj: primitive_project_work.nanojoules(),
        },
        investment_policy,
    );
    #[cfg(not(test))]
    let primitive_frontier = planning::primitive_power_decision_frontier(
        registries,
        &state,
        raw,
        shaped,
        store_definition,
        capacity_nj,
        investment_policy,
    );
    assert_eq!(
        plan.minimum_return_ppm,
        investment_policy.minimum_return_ppm()
    );
    let mut settlement_state = AppState::new();
    let settlement_roots = [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ];
    let mut settlement_provider_equipment =
        power_provider_equipment_for_roots(registries, &settlement_roots);
    settlement_provider_equipment.push(EQUIPMENT_TIMBER_SASH_SAWMILL);
    let settlement_requirements = power_raw_opportunity(
        registries,
        &settlement_roots,
        &settlement_provider_equipment,
        &[ENERGY_TIMBER_FRAME_FLYWHEEL_BANK],
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
        "settlement power opportunity",
    );
    let (settlement_raw, settlement_raw_capacity) = seed_raw_opportunity(
        registries,
        &mut settlement_state,
        settlement_requirements,
        "settlement power opportunity",
    );
    let settlement_shaped = add_solid_stockpile(&mut settlement_state, settlement_raw_capacity);
    let settlement_capacity_nj = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .map(|definition| definition.capacity().nanojoules())
        .unwrap_or_else(|| panic!("settlement flywheel bank definition disappeared"));
    let settlement_process = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement power project saw process disappeared"));
    let settlement_batch_mass = registries
        .crafting()
        .get_manual(settlement_process.transform())
        .map(|transform| transform.input_mass())
        .unwrap_or_else(|| panic!("settlement power project manual saw transform disappeared"));
    let settlement_available_mass = Mass::from_milligrams(
        settlement_batch_mass
            .milligrams()
            .checked_mul(SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES)
            .unwrap_or_else(|| panic!("settlement power available workload overflowed")),
    );
    let settlement_feed = add_solid_stockpile(&mut settlement_state, settlement_available_mass);
    seed_lot(
        registries,
        &mut settlement_state,
        settlement_feed,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        settlement_available_mass,
        ROOM_TEMPERATURE,
    );
    let settlement_output = add_solid_stockpile(&mut settlement_state, settlement_available_mass);
    let settlement_service_replacement =
        add_solid_stockpile(&mut settlement_state, settlement_raw_capacity);
    let settlement_service_spent =
        add_solid_stockpile(&mut settlement_state, settlement_raw_capacity);
    let settlement_provisions = seed_power_project_provisions(registries, &mut settlement_state);
    admit_stationary_player_with_survival_start(
        registries,
        &mut settlement_state,
        &[
            settlement_raw,
            settlement_feed,
            settlement_shaped,
            settlement_output,
            settlement_service_replacement,
            settlement_service_spent,
            settlement_provisions.food,
            settlement_provisions.enclosure_material,
        ],
        &[settlement_provisions.water],
        settlement_survival_start,
        "settlement power-provider",
    );
    let (settlement_project_mass, settlement_project_work) =
        declared_settlement_lumber_project(registries, seed);
    assert!(settlement_project_mass <= settlement_available_mass);
    let settlement_consumer = build_settlement_power_consumer(
        registries,
        &mut settlement_state,
        settlement_raw,
        settlement_shaped,
        settlement_feed,
        settlement_output,
    );
    let settlement_matter_before = calculate_matter_accounting(&settlement_state)
        .unwrap_or_else(|error| panic!("settlement power initial matter audit failed: {error}"))
        .total();
    let settlement_fluid_before = calculate_fluid_volume_accounting(&settlement_state)
        .unwrap_or_else(|error| panic!("settlement power initial fluid audit failed: {error}"))
        .total();
    let copper_policy = settlement_copper_policy(case);
    let settlement_plan = settlement_power_plan(
        registries,
        &settlement_state,
        settlement_raw,
        settlement_shaped,
        planning::SettlementPowerProject {
            capacity_nj: settlement_capacity_nj,
            declared_mass: settlement_project_mass,
            declared_work_nj: settlement_project_work.nanojoules(),
        },
        copper_policy,
        investment_policy,
    );
    #[cfg(not(test))]
    let settlement_frontier = planning::settlement_power_decision_frontier(
        registries,
        &settlement_state,
        settlement_raw,
        settlement_shaped,
        settlement_capacity_nj,
        copper_policy,
        investment_policy,
    );
    assert_eq!(
        settlement_plan.minimum_return_ppm,
        investment_policy.minimum_return_ppm()
    );

    // Experience the complete project through the actor-selected provider. The matched branches
    // below still isolate provider deltas over one productive cycle, while these selected runs pay
    // every repeated charge and any consumer maintenance that the declared project actually reaches.
    let primitive_resources = ProjectExecutionResources::new(
        raw,
        shaped,
        primitive_service_replacement,
        primitive_service_spent,
        primitive_provisions,
        matter_before,
        fluid_before,
    );
    let primitive_selected = execute_selected_primitive_project(
        registries,
        &state,
        primitive_resources,
        plan,
        primitive_consumer,
    );
    #[cfg(test)]
    let _ = &primitive_selected;
    #[cfg(not(test))]
    let primitive_crank_project = execute_selected_primitive_project(
        registries,
        &state,
        primitive_resources,
        plan.with_reference_choice(PrimitivePowerChoice::Crank),
        primitive_consumer,
    );
    #[cfg(not(test))]
    let primitive_treadle_project = execute_selected_primitive_project(
        registries,
        &state,
        primitive_resources,
        plan.with_reference_choice(PrimitivePowerChoice::Treadle),
        primitive_consumer,
    );
    #[cfg(not(test))]
    let primitive_walking_project = execute_selected_primitive_project(
        registries,
        &state,
        primitive_resources,
        plan.with_reference_choice(PrimitivePowerChoice::WalkingWheel),
        primitive_consumer,
    );
    let settlement_resources = ProjectExecutionResources::new(
        settlement_raw,
        settlement_shaped,
        settlement_service_replacement,
        settlement_service_spent,
        settlement_provisions,
        settlement_matter_before,
        settlement_fluid_before,
    );
    let settlement_selected = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        settlement_plan,
        settlement_consumer,
    );
    #[cfg(test)]
    let _ = &settlement_selected;
    #[cfg(not(test))]
    let settlement_stone_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        settlement_plan.with_reference_choice(SettlementPowerChoice::StoneCrank),
        settlement_consumer,
    );
    #[cfg(not(test))]
    let settlement_copper_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        settlement_plan.with_reference_choice(SettlementPowerChoice::CopperCrank),
        settlement_consumer,
    );
    #[cfg(not(test))]
    let settlement_treadle_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        settlement_plan.with_reference_choice(SettlementPowerChoice::Treadle),
        settlement_consumer,
    );
    #[cfg(not(test))]
    let settlement_treadle_dynamo_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        settlement_plan.with_reference_choice(SettlementPowerChoice::TreadleDynamo),
        settlement_consumer,
    );
    #[cfg(not(test))]
    let settlement_double_wound_treadle_dynamo_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        settlement_plan.with_reference_choice(SettlementPowerChoice::DoubleWoundTreadleDynamo),
        settlement_consumer,
    );
    #[cfg(not(test))]
    let settlement_walking_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        settlement_plan.with_reference_choice(SettlementPowerChoice::WalkingWheel),
        settlement_consumer,
    );
    #[cfg(not(test))]
    {
        let actual_crank_attention = primitive_crank_project.active_attention_ticks();
        let actual_treadle_attention = primitive_treadle_project.active_attention_ticks();
        let actual_treadle_return_floor = investment_policy.minimum_attention_return(
            plan.crank_build.attention_ticks,
            plan.treadle_build.attention_ticks,
        );
        let actual_treadle_is_eligible = clears_attention_return(
            actual_crank_attention,
            actual_treadle_attention,
            actual_treadle_return_floor,
        );
        if plan.choice == PrimitivePowerChoice::Crank.provider() {
            assert!(
                !actual_treadle_is_eligible,
                "primitive provider planner kept the crank even though the fully executed treadle saved enough attention to repay its declared capital-return floor"
            );
        } else if plan.choice == PrimitivePowerChoice::Treadle.provider() {
            assert!(
                actual_treadle_is_eligible,
                "primitive provider planner selected the treadle but its fully executed project did not repay the declared capital-return floor"
            );
        }
    }
    #[cfg(not(test))]
    let primitive_actual_attention_minimum = [
        primitive_crank_project.active_attention_ticks(),
        primitive_treadle_project.active_attention_ticks(),
        primitive_walking_project.active_attention_ticks(),
    ]
    .into_iter()
    .min()
    .unwrap_or_else(|| unreachable!("primitive provider comparison is nonempty"));
    #[cfg(not(test))]
    let primitive_actual_attention_best = {
        let best = [
            (PrimitivePowerChoice::Crank, primitive_crank_project),
            (PrimitivePowerChoice::Treadle, primitive_treadle_project),
            (
                PrimitivePowerChoice::WalkingWheel,
                primitive_walking_project,
            ),
        ]
        .into_iter()
        .filter(|(_, outcome)| {
            outcome.active_attention_ticks() == primitive_actual_attention_minimum
        })
        .map(|(choice, _)| choice)
        .collect::<Vec<_>>();
        if best.len() == 1 {
            best[0].label()
        } else {
            "tie"
        }
    };
    #[cfg(not(test))]
    let primitive_selected_reference_attention_delta =
        i128::from(primitive_selected.active_attention_ticks())
            - i128::from(primitive_actual_attention_minimum);
    #[cfg(not(test))]
    let settlement_actual_attention_minimum = [
        settlement_stone_project.active_attention_ticks(),
        settlement_copper_project.active_attention_ticks(),
        settlement_treadle_project.active_attention_ticks(),
        settlement_treadle_dynamo_project.active_attention_ticks(),
        settlement_double_wound_treadle_dynamo_project.active_attention_ticks(),
        settlement_walking_project.active_attention_ticks(),
    ]
    .into_iter()
    .min()
    .unwrap_or_else(|| unreachable!("settlement provider comparison is nonempty"));
    #[cfg(not(test))]
    let settlement_actual_attention_best = {
        let best = [
            (SettlementPowerChoice::StoneCrank, settlement_stone_project),
            (
                SettlementPowerChoice::CopperCrank,
                settlement_copper_project,
            ),
            (SettlementPowerChoice::Treadle, settlement_treadle_project),
            (
                SettlementPowerChoice::TreadleDynamo,
                settlement_treadle_dynamo_project,
            ),
            (
                SettlementPowerChoice::DoubleWoundTreadleDynamo,
                settlement_double_wound_treadle_dynamo_project,
            ),
            (
                SettlementPowerChoice::WalkingWheel,
                settlement_walking_project,
            ),
        ]
        .into_iter()
        .filter(|(_, outcome)| {
            outcome.active_attention_ticks() == settlement_actual_attention_minimum
        })
        .map(|(choice, _)| choice)
        .collect::<Vec<_>>();
        if best.len() == 1 {
            best[0].label()
        } else {
            "tie"
        }
    };
    #[cfg(not(test))]
    let settlement_allowed_attention = [
        (SettlementPowerChoice::StoneCrank, settlement_stone_project),
        (
            SettlementPowerChoice::CopperCrank,
            settlement_copper_project,
        ),
        (SettlementPowerChoice::Treadle, settlement_treadle_project),
        (
            SettlementPowerChoice::TreadleDynamo,
            settlement_treadle_dynamo_project,
        ),
        (
            SettlementPowerChoice::DoubleWoundTreadleDynamo,
            settlement_double_wound_treadle_dynamo_project,
        ),
        (
            SettlementPowerChoice::WalkingWheel,
            settlement_walking_project,
        ),
    ]
    .into_iter()
    .filter(|(choice, _)| {
        copper_policy == SettlementCopperPolicy::SpendAvailable || !choice.uses_copper(registries)
    })
    .collect::<Vec<_>>();
    #[cfg(not(test))]
    let settlement_allowed_attention_minimum = settlement_allowed_attention
        .iter()
        .map(|(_, outcome)| outcome.active_attention_ticks())
        .min()
        .unwrap_or_else(|| unreachable!("settlement copper policy leaves playable providers"));
    #[cfg(not(test))]
    let settlement_allowed_attention_best = {
        let best = settlement_allowed_attention
            .iter()
            .filter(|(_, outcome)| {
                outcome.active_attention_ticks() == settlement_allowed_attention_minimum
            })
            .map(|(choice, _)| *choice)
            .collect::<Vec<_>>();
        if best.len() == 1 {
            best[0].label()
        } else {
            "tie"
        }
    };
    #[cfg(not(test))]
    let settlement_selected_reference_attention_delta =
        i128::from(settlement_selected.active_attention_ticks())
            - i128::from(settlement_allowed_attention_minimum);
    #[cfg(not(test))]
    reviewln!(
        "POWER PROJECT EXPERIENCE seed=0x{seed:016X} sample={} era=primitive survival-start={} selected={} declared=[work:{}nJ pristine-charge-events:{} consumer-projected-batches:{} consumer-projected-services:{} project-cache=[food:{}mg preservation:{}ppm water:{}uL]] executed=[charge-events:{} survival-limited-batches:{} active-attention:{}t provider-attention:{}t consumer-runtime:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg] provisioning=[stops:{} attention:{}t drinks:{} volume:{}uL meals:{} mass:{}mg] elapsed:{}t reserves=[start:{}nJ/{}uL end:{}nJ/{}uL]] condition=[provider:{}ppm consumer:{}ppm] reference-counterfactual=[crank-active-attention:{}t treadle-active-attention:{}t walking-wheel-active-attention:{}t attention-best:{} selected-vs-reference-attention-delta:{:+}t] evidence=complete-selected-project-canonical",
        case.role().label(),
        primitive_survival_start.label(),
        plan.choice.primitive_label(),
        plan.declared_work_nj,
        plan.charge_events,
        plan.consumer_projected_batches,
        plan.consumer_projected_services,
        primitive_provisions.food_supply_mg,
        primitive_provisions.food_preservation_ppm,
        primitive_provisions.water_supply_ul,
        primitive_selected.charge_events,
        primitive_selected.survival_limited_batches,
        primitive_selected.active_attention_ticks(),
        primitive_selected.provider_attention_ticks,
        primitive_selected.consumer_ticks,
        primitive_selected.consumer_services,
        primitive_selected.service_preparation_ticks,
        primitive_selected.service_ticks,
        primitive_selected.replacement_mass_mg,
        primitive_selected.provisioning_stops,
        primitive_selected.provisioning_attention_ticks,
        primitive_selected.drink_actions,
        primitive_selected.drink_volume_ul,
        primitive_selected.meal_actions,
        primitive_selected.meal_mass_mg,
        primitive_selected.elapsed_ticks,
        primitive_selected.initial_metabolic_nj,
        primitive_selected.initial_hydration_ul,
        primitive_selected.final_metabolic_nj,
        primitive_selected.final_hydration_ul,
        primitive_selected.provider_condition_ppm,
        primitive_selected.consumer_condition_ppm,
        primitive_crank_project.active_attention_ticks(),
        primitive_treadle_project.active_attention_ticks(),
        primitive_walking_project.active_attention_ticks(),
        primitive_actual_attention_best,
        primitive_selected_reference_attention_delta,
    );
    #[cfg(not(test))]
    reviewln!(
        "POWER PROJECT EXPERIENCE seed=0x{seed:016X} sample={} era=settlement context=later-scaled-workshop buffer-stage=timber-bank survival-start={} selected={} copper-policy={} declared=[work:{}nJ pristine-charge-events:{} project-cache=[food:{}mg preservation:{}ppm water:{}uL]] executed=[charge-events:{} survival-limited-batches:{} active-attention:{}t provider-attention:{}t consumer-runtime:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg] provisioning=[stops:{} attention:{}t drinks:{} volume:{}uL meals:{} mass:{}mg] elapsed:{}t reserves=[start:{}nJ/{}uL end:{}nJ/{}uL]] condition=[provider:{}ppm consumer:{}ppm] reference-counterfactual=[stone-crank-active-attention:{}t copper-crank-active-attention:{}t treadle-active-attention:{}t treadle-dynamo-active-attention:{}t double-wound-treadle-dynamo-active-attention:{}t walking-wheel-active-attention:{}t attention-best:{} allowed-attention-best:{} selected-vs-reference-attention-delta:{:+}t] evidence=complete-selected-project-canonical",
        case.role().label(),
        settlement_survival_start.label(),
        settlement_plan.choice.settlement_label(),
        copper_policy.label(),
        settlement_plan.declared_work_nj,
        settlement_plan.charge_events,
        settlement_provisions.food_supply_mg,
        settlement_provisions.food_preservation_ppm,
        settlement_provisions.water_supply_ul,
        settlement_selected.charge_events,
        settlement_selected.survival_limited_batches,
        settlement_selected.active_attention_ticks(),
        settlement_selected.provider_attention_ticks,
        settlement_selected.consumer_ticks,
        settlement_selected.consumer_services,
        settlement_selected.service_preparation_ticks,
        settlement_selected.service_ticks,
        settlement_selected.replacement_mass_mg,
        settlement_selected.provisioning_stops,
        settlement_selected.provisioning_attention_ticks,
        settlement_selected.drink_actions,
        settlement_selected.drink_volume_ul,
        settlement_selected.meal_actions,
        settlement_selected.meal_mass_mg,
        settlement_selected.elapsed_ticks,
        settlement_selected.initial_metabolic_nj,
        settlement_selected.initial_hydration_ul,
        settlement_selected.final_metabolic_nj,
        settlement_selected.final_hydration_ul,
        settlement_selected.provider_condition_ppm,
        settlement_selected.consumer_condition_ppm,
        settlement_stone_project.active_attention_ticks(),
        settlement_copper_project.active_attention_ticks(),
        settlement_treadle_project.active_attention_ticks(),
        settlement_treadle_dynamo_project.active_attention_ticks(),
        settlement_double_wound_treadle_dynamo_project.active_attention_ticks(),
        settlement_walking_project.active_attention_ticks(),
        settlement_actual_attention_best,
        settlement_allowed_attention_best,
        settlement_selected_reference_attention_delta,
    );

    // Matched arms inherit the same actor-visible state. Execution owns projection agreement,
    // conservation, and trusted-load validity before reporting compares outcomes.
    #[cfg_attr(
        test,
        allow(
            unused_variables,
            reason = "second-charge and matched consumer timings are report diagnostics; execution validates them before returning"
        )
    )]
    let PrimitiveComparison {
        crank_build,
        crank_drive_build,
        crank_charge,
        treadle_build,
        treadle_drive_build,
        treadle_charge,
        crank_second_charge,
        treadle_second_charge,
        crank_consumer_ticks,
        treadle_consumer_ticks,
        crank_residual_mg,
        treadle_residual_mg,
    } = execute_primitive_comparison(
        registries,
        &state,
        raw,
        shaped,
        matter_before,
        plan,
        primitive_consumer,
    );
    #[cfg_attr(
        test,
        allow(
            unused_variables,
            reason = "second-charge and matched consumer timings are report diagnostics; execution validates them before returning"
        )
    )]
    let SettlementComparison {
        settlement_treadle_build,
        settlement_treadle_drive_build,
        settlement_treadle_charge,
        walking_build,
        walking_drive_build,
        walking_charge,
        settlement_treadle_second_charge,
        walking_second_charge,
        settlement_treadle_consumer_ticks,
        walking_consumer_ticks,
    } = execute_settlement_comparison(
        registries,
        &settlement_state,
        settlement_raw,
        settlement_shaped,
        settlement_matter_before,
        settlement_plan,
        settlement_consumer,
    );

    let settlement_treadle_build_attention = settlement_treadle_build
        .attention_ticks
        .checked_add(settlement_treadle_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("settlement treadle build attention overflowed"));
    let walking_build_attention = walking_build
        .attention_ticks
        .checked_add(walking_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("walking-wheel build attention overflowed"));
    let settlement_treadle_build_mass = settlement_treadle_build
        .input_mass_mg
        .checked_add(settlement_treadle_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("settlement treadle build mass overflowed"));
    let walking_build_mass = walking_build
        .input_mass_mg
        .checked_add(walking_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("walking-wheel build mass overflowed"));
    assert!(
        walking_build_mass > settlement_treadle_build_mass,
        "walking-wheel route must retain a larger timber investment than the treadle route"
    );
    assert!(
        walking_charge.attention_ticks < settlement_treadle_charge.attention_ticks,
        "walking wheel must save settlement charge attention"
    );
    assert!(
        walking_build_attention > settlement_treadle_build_attention,
        "walking wheel must cost more build attention than the treadle"
    );
    #[cfg(not(test))]
    let settlement_charge_saving =
        settlement_treadle_charge.attention_ticks - walking_charge.attention_ticks;
    #[cfg(not(test))]
    let settlement_build_attention_delta =
        walking_build_attention - settlement_treadle_build_attention;
    #[cfg(not(test))]
    let settlement_break_even_charges =
        settlement_build_attention_delta.div_ceil(settlement_charge_saving);
    #[cfg(not(test))]
    let settlement_market_frontier = settlement_frontier_label(&settlement_frontier);
    assert_eq!(
        settlement_plan.declared_work_nj,
        settlement_project_work.nanojoules()
    );
    #[cfg(not(test))]
    reviewln!(
        "POWER SETTLEMENT seed=0x{seed:016X} sample={} context=later-scaled-workshop buffer-stage=timber-bank workload-source=declared-consumer-project project=[consumer:powered-saw feed:{}mg work:{}nJ charge-events:{}] buffer:{}nJ decision=[selected:{} copper-policy:{} policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t projected-attention-treadle:{}t projected-attention-walking:{}t choice-frozen-before-action:true] treadle=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] walking-wheel=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] productive-cycle=[consumer:powered-saw treadle:{}t walking:{}t] projected-provider-lifecycle=[treadle:body:{}nJ/{}uL condition:{}ppm walking-wheel:body:{}nJ/{}uL condition:{}ppm] comparison=[charge-saving:{}t metabolic-saving:{}nJ pristine-rate-break-even:{}charges market-frontier:{} provider-lifecycle=condition-carried-no-service] selected-project=[charge-events:{} provider-attention:{}t consumer:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg policy:service-at-critical] elapsed:{}t reserves=[start:{}nJ/{}uL end:{}nJ/{}uL] provider-condition:{}ppm consumer-condition:{}ppm] evidence=[build+charge+productive-discharge+recharge:executed selected-project:executed comparator-lifecycle:projected-canonical consumer:powered-saw] matter=conserved",
        case.role().label(),
        settlement_project_mass.milligrams(),
        settlement_project_work.nanojoules(),
        settlement_plan.charge_events,
        settlement_capacity_nj,
        settlement_plan.choice.settlement_label(),
        copper_policy.label(),
        settlement_plan.minimum_return_ppm,
        settlement_plan.minimum_attention_return_ticks,
        settlement_plan.treadle_lifecycle_attention,
        settlement_plan.walking_lifecycle_attention,
        settlement_treadle_build_mass,
        settlement_treadle_build_attention,
        settlement_plan.treadle_build.metabolic_nj,
        settlement_plan.treadle_build.hydration_ul,
        settlement_treadle_charge.attention_ticks,
        settlement_treadle_second_charge.attention_ticks,
        settlement_treadle_charge.metabolic_nj,
        settlement_treadle_charge.hydration_ul,
        settlement_treadle_charge.condition_after_ppm,
        walking_build_mass,
        walking_build_attention,
        settlement_plan.walking_build.metabolic_nj,
        settlement_plan.walking_build.hydration_ul,
        walking_charge.attention_ticks,
        walking_second_charge.attention_ticks,
        walking_charge.metabolic_nj,
        walking_charge.hydration_ul,
        walking_charge.condition_after_ppm,
        settlement_treadle_consumer_ticks,
        walking_consumer_ticks,
        settlement_plan.treadle_lifecycle_metabolic_nj,
        settlement_plan.treadle_lifecycle_hydration_ul,
        settlement_plan
            .treadle_lifecycle_condition
            .parts_per_million(),
        settlement_plan.walking_lifecycle_metabolic_nj,
        settlement_plan.walking_lifecycle_hydration_ul,
        settlement_plan
            .walking_lifecycle_condition
            .parts_per_million(),
        settlement_charge_saving,
        settlement_treadle_charge
            .metabolic_nj
            .checked_sub(walking_charge.metabolic_nj)
            .unwrap_or_else(|| panic!("walking wheel must reduce charge metabolism")),
        settlement_break_even_charges,
        settlement_market_frontier,
        settlement_selected.charge_events,
        settlement_selected.provider_attention_ticks,
        settlement_selected.consumer_ticks,
        settlement_selected.consumer_services,
        settlement_selected.service_preparation_ticks,
        settlement_selected.service_ticks,
        settlement_selected.replacement_mass_mg,
        settlement_selected.elapsed_ticks,
        settlement_selected.initial_metabolic_nj,
        settlement_selected.initial_hydration_ul,
        settlement_selected.final_metabolic_nj,
        settlement_selected.final_hydration_ul,
        settlement_selected.provider_condition_ppm,
        settlement_selected.consumer_condition_ppm,
    );

    #[cfg(not(test))]
    let charge_attention_reduction_ppm = u64::try_from(
        u128::from(
            crank_charge
                .attention_ticks
                .checked_sub(treadle_charge.attention_ticks)
                .unwrap_or_else(|| panic!("treadle must reduce primitive charge attention")),
        )
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("power provider attention reduction overflowed"))
            / u128::from(crank_charge.attention_ticks),
    )
    .unwrap_or_else(|_| panic!("power provider attention reduction exceeds u64"));
    #[cfg(not(test))]
    let crank_build_attention = crank_build
        .attention_ticks
        .checked_add(crank_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("power provider crank build attention overflowed"));
    let crank_build_mass_mg = crank_build
        .input_mass_mg
        .checked_add(crank_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("power provider crank build mass overflowed"));
    #[cfg(not(test))]
    let treadle_build_attention = treadle_build
        .attention_ticks
        .checked_add(treadle_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("power provider treadle build attention overflowed"));
    let treadle_build_mass_mg = treadle_build
        .input_mass_mg
        .checked_add(treadle_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("power provider treadle build mass overflowed"));
    assert!(
        treadle_build_mass_mg > crank_build_mass_mg,
        "the treadle route must remain a heavier material investment than the crank route"
    );
    // Initial-rate estimate, not executed lifetime payback: repeat charges incur wear and
    // eventually service. Keep that uncertainty visible rather than projecting one fresh
    // charge into a guaranteed lifetime result.
    let charge_saving_per_job_ticks = crank_charge
        .attention_ticks
        .checked_sub(treadle_charge.attention_ticks)
        .unwrap_or_else(|| {
            panic!("power provider treadle must save charge attention on the same job")
        });
    assert!(
        charge_saving_per_job_ticks > 0,
        "the treadle's higher charging throughput must repay attention on the same flywheel job"
    );
    #[cfg(not(test))]
    let build_attention_delta_ticks = treadle_build_attention.saturating_sub(crank_build_attention);
    #[cfg(not(test))]
    let break_even_charges = build_attention_delta_ticks.div_ceil(charge_saving_per_job_ticks);
    #[cfg(not(test))]
    let primitive_market_frontier = primitive_frontier_label(&primitive_frontier);
    assert_eq!(plan.declared_work_nj, primitive_project_work.nanojoules());
    let crank_package = crank_build.checked_add(crank_drive_build, "crank package");
    let treadle_package = treadle_build.checked_add(treadle_drive_build, "treadle package");
    let crank_embodied = crank_package.embodied_mass_mg;
    let treadle_embodied = treadle_package.embodied_mass_mg;
    let crank_residual = crank_residual_mg;
    let treadle_residual = treadle_residual_mg;
    assert_eq!(
        crank_build_mass_mg,
        crank_embodied
            .checked_add(crank_residual)
            .unwrap_or_else(|| panic!("crank material reconciliation overflowed")),
        "crank raw bill must reconcile equipment, flywheel, surplus and shaping residue"
    );
    assert_eq!(
        treadle_build_mass_mg,
        treadle_embodied
            .checked_add(treadle_residual)
            .unwrap_or_else(|| panic!("treadle material reconciliation overflowed")),
        "treadle raw bill must reconcile equipment, flywheel, surplus and shaping residue"
    );
    assert!(
        crank_residual > 0 && treadle_residual > 0,
        "these shaped builds must expose their real surplus/residue rather than just assembly mass"
    );
    #[cfg(not(test))]
    reviewln!(
        "POWER BUILD BILL seed=0x{seed:016X} basis=executed-raw-withdrawal crank=[raw:{:.3}kg embodied:{:.3}kg residual:{:.3}kg build:{} charge:{} charge-body:{:.2}kJ] treadle=[raw:{:.3}kg embodied:{:.3}kg residual:{:.3}kg build:{} charge:{} charge-body:{:.2}kJ] buffer:{:.0}J break-even:{}charges estimate=initial-charge-rate-excludes-future-wear-and-service",
        crank_build_mass_mg as f64 / 1_000_000.0,
        crank_embodied as f64 / 1_000_000.0,
        crank_residual as f64 / 1_000_000.0,
        format_physical_duration(registries, crank_build_attention),
        format_physical_duration(registries, crank_charge.attention_ticks),
        crank_charge.metabolic_nj as f64 / 1_000_000_000_000.0,
        treadle_build_mass_mg as f64 / 1_000_000.0,
        treadle_embodied as f64 / 1_000_000.0,
        treadle_residual as f64 / 1_000_000.0,
        format_physical_duration(registries, treadle_build_attention),
        format_physical_duration(registries, treadle_charge.attention_ticks),
        treadle_charge.metabolic_nj as f64 / 1_000_000_000_000.0,
        capacity_nj as f64 / 1_000_000_000.0,
        break_even_charges,
    );
    #[cfg(not(test))]
    reviewln!(
        "POWER PROVIDER EXPERIENCE seed=0x{seed:016X} sample={} workload-source={} project=[consumer:stone-crusher feed:{}mg work:{}nJ declared-charge-events:{} consumer-projected-batches:{} projected-services:{}] buffer:{}nJ decision=[selected:{} policy=attention-first-with-minimum-investment-return minimum-return:{}ppm minimum-attention-return:{}t projected-attention-crank:{}t projected-attention-treadle:{}t choice-frozen-before-action:true] crank=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] treadle=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] productive-cycle=[consumer:stone-crusher crank:{}t treadle:{}t] projected-provider-lifecycle=[crank:body:{}nJ/{}uL condition:{}ppm treadle:body:{}nJ/{}uL condition:{}ppm] comparison=[basis:matched-starting-state charge-attention-reduction:{}ppm build-mass-crank:{}mg build-mass-treadle:{}mg metabolic-crank:{}nJ metabolic-treadle:{}nJ build-attention-crank:{}t build-attention-treadle:{}t charge-crank:{}t charge-treadle:{}t charge-saving:{}t pristine-rate-break-even:{} market-frontier:{} provider-lifecycle=consumer-batches+provider-condition] selected-project=[charge-events:{} provider-attention:{}t consumer:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg policy:service-at-critical] elapsed:{}t reserves=[start:{}nJ/{}uL end:{}nJ/{}uL] provider-condition:{}ppm consumer-condition:{}ppm] evidence=[build+charge+productive-discharge+recharge:executed selected-project:executed comparator-lifecycle:projected-canonical consumer:stone-crusher] matter=conserved",
        case.role().label(),
        _primitive_workload.label(),
        primitive_project_mass.milligrams(),
        primitive_project_work.nanojoules(),
        plan.charge_events,
        plan.consumer_projected_batches,
        plan.consumer_projected_services,
        capacity_nj,
        plan.choice.primitive_label(),
        plan.minimum_return_ppm,
        plan.minimum_attention_return_ticks,
        plan.crank_lifecycle_attention,
        plan.treadle_lifecycle_attention,
        crank_build_mass_mg,
        crank_build_attention,
        plan.crank_build.metabolic_nj,
        plan.crank_build.hydration_ul,
        crank_charge.attention_ticks,
        crank_second_charge.attention_ticks,
        crank_charge.metabolic_nj,
        crank_charge.hydration_ul,
        crank_charge.condition_after_ppm,
        treadle_build_mass_mg,
        treadle_build_attention,
        plan.treadle_build.metabolic_nj,
        plan.treadle_build.hydration_ul,
        treadle_charge.attention_ticks,
        treadle_second_charge.attention_ticks,
        treadle_charge.metabolic_nj,
        treadle_charge.hydration_ul,
        treadle_charge.condition_after_ppm,
        crank_consumer_ticks,
        treadle_consumer_ticks,
        plan.crank_lifecycle_metabolic_nj,
        plan.crank_lifecycle_hydration_ul,
        plan.crank_lifecycle_condition.parts_per_million(),
        plan.treadle_lifecycle_metabolic_nj,
        plan.treadle_lifecycle_hydration_ul,
        plan.treadle_lifecycle_condition.parts_per_million(),
        charge_attention_reduction_ppm,
        crank_build_mass_mg,
        treadle_build_mass_mg,
        crank_charge.metabolic_nj,
        treadle_charge.metabolic_nj,
        crank_build_attention,
        treadle_build_attention,
        crank_charge.attention_ticks,
        treadle_charge.attention_ticks,
        charge_saving_per_job_ticks,
        break_even_charges,
        primitive_market_frontier,
        primitive_selected.charge_events,
        primitive_selected.provider_attention_ticks,
        primitive_selected.consumer_ticks,
        primitive_selected.consumer_services,
        primitive_selected.service_preparation_ticks,
        primitive_selected.service_ticks,
        primitive_selected.replacement_mass_mg,
        primitive_selected.elapsed_ticks,
        primitive_selected.initial_metabolic_nj,
        primitive_selected.initial_hydration_ul,
        primitive_selected.final_metabolic_nj,
        primitive_selected.final_hydration_ul,
        primitive_selected.provider_condition_ppm,
        primitive_selected.consumer_condition_ppm,
    );
}
