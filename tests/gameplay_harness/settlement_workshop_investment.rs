//! Full capital-crossover experiences for later settlement workshop machines.

use std::collections::BTreeMap;

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_SPRING_POLE_LATHE, EQUIPMENT_TIMBER_TREADLE_GRINDSTONE, FORM_CHIP,
    FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_SCRAP, FORM_TOOL,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_GRIND_STONE_SCRAP_TOOL, PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
    PROCESS_POWER_TURN_TIMBER_FLYWHEEL, PROCESS_SHAPE_TIMBER_FLYWHEEL, build_registries,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    ManualCraftRequest, PoweredCraftRequest, resolve_manual_craft, validate_start_powered_craft,
};
use deep_hearth::energy::{EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::inventory::{MaterialLotId, StockpileId, StockpileStorageProfile};
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

use super::capital_investment_crossover::first_attention_return_crossover;
use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_craft_topology_planning::manual_craft_topology_plan_for_output_from_inputs;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::powered_craft_planning::{
    AuthoredPoweredCraftBatch, PoweredCraftSequenceProjection, authored_batch,
    project_powered_craft_sequence,
};
use super::production_timing::finish_uninterrupted_production_job;
use super::settlement_generation::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, crossover_workloads,
};
use super::settlement_power_planning::{
    ManualPowerSequenceProjection, project_manual_power_workload,
};

fn seed_material(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> MaterialLotId {
    seed_lot(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        ROOM_TEMPERATURE,
    )
}

const UPGRADE_RAW_INPUTS: [CommodityKey; 3] = [
    CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
    CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
    CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
];

#[derive(Clone, Copy)]
struct UpgradeSetupStep {
    process: ProcessId,
    batches: u64,
    equipment: Option<EquipmentId>,
}

fn upgrade_setup_steps(
    registries: &Registries,
    target: EquipmentDefinitionId,
    handle_equipment: Option<EquipmentId>,
    context: &'static str,
) -> Vec<UpgradeSetupStep> {
    let additions = registries
        .equipment()
        .get_equipment(target)
        .and_then(|definition| definition.upgrade_profile())
        .map(|profile| profile.additions())
        .unwrap_or_else(|| panic!("{context} target lost its additive upgrade profile"));
    additions
        .inputs()
        .iter()
        .map(|input| {
            let (definition, batches) = manual_craft_topology_plan_for_output_from_inputs(
                registries,
                input.commodity(),
                input.mass(),
                &UPGRADE_RAW_INPUTS,
                context,
            );
            UpgradeSetupStep {
                process: definition.process(),
                batches,
                equipment: (input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))
                    .then_some(handle_equipment)
                    .flatten(),
            }
        })
        .collect()
}

pub(super) fn seed_upgrade_raw(
    registries: &Registries,
    state: &mut AppState,
    target: EquipmentDefinitionId,
    context: &'static str,
) -> (StockpileId, StockpileId) {
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    for step in upgrade_setup_steps(registries, target, None, context) {
        let definition = registries
            .crafting()
            .get_manual(step.process)
            .unwrap_or_else(|| panic!("{context} setup process disappeared"));
        let mass = Mass::from_milligrams(
            definition
                .input_mass()
                .milligrams()
                .checked_mul(step.batches)
                .unwrap_or_else(|| panic!("{context} raw setup mass overflowed")),
        );
        let total = requirements.entry(definition.input()).or_insert(Mass::ZERO);
        *total = total
            .checked_add(mass)
            .unwrap_or_else(|| panic!("{context} raw requirement overflowed"));
    }
    let capacity = requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .filter(|mass| !mass.is_zero())
        .unwrap_or_else(|| panic!("{context} has no primitive setup matter"));
    let raw = seed_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in requirements {
        seed_material(registries, state, raw, commodity, mass);
    }
    let parts = seed_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    (raw, parts)
}

fn setup_request(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    process: ProcessId,
    batches: u64,
    equipment: Option<EquipmentId>,
    context: &'static str,
) -> ManualCraftRequest {
    let request = select_manual_craft_request(registries, state, process, raw, batches, context);
    match equipment {
        Some(equipment) => request.with_equipment(equipment),
        None => request,
    }
}

fn project_upgrade_setup(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    target: EquipmentDefinitionId,
    handle_equipment: Option<EquipmentId>,
    context: &'static str,
) -> u64 {
    upgrade_setup_steps(registries, target, handle_equipment, context)
        .into_iter()
        .map(|step| {
            let request = setup_request(
                registries,
                state,
                raw,
                step.process,
                step.batches,
                step.equipment,
                context,
            );
            resolve_manual_craft(registries, state, &request)
                .unwrap_or_else(|error| panic!("{context} projection failed: {error}"))
                .duration()
                .value()
        })
        .try_fold(0_u64, u64::checked_add)
        .unwrap_or_else(|| panic!("settlement flywheel-upgrade setup attention overflowed"))
}

fn execute_upgrade_setup(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    target: EquipmentDefinitionId,
    handle_equipment: Option<EquipmentId>,
    context: &'static str,
) -> u64 {
    upgrade_setup_steps(registries, target, handle_equipment, context)
        .into_iter()
        .map(|step| {
            let request = setup_request(
                registries,
                state,
                raw,
                step.process,
                step.batches,
                step.equipment,
                context,
            );
            execute_manual_craft(registries, state, request, parts, context).value()
        })
        .try_fold(0_u64, u64::checked_add)
        .unwrap_or_else(|| panic!("settlement flywheel-upgrade execution attention overflowed"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredProjectOutcome {
    pub(super) charge_attention: u64,
    pub(super) delegated_ticks: u64,
    pub(super) charge_events: u64,
    pub(super) maximum_leg_batches: u64,
}

pub(super) struct PoweredProjectProjection {
    pub(super) sequence: PoweredCraftSequenceProjection,
    pub(super) charging: ManualPowerSequenceProjection,
}

pub(super) fn project_powered_project(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    machine: EquipmentDefinitionId,
    machine_condition: deep_hearth::maintenance::Condition,
    crank_condition: deep_hearth::maintenance::Condition,
    drive: EnergyStoreId,
    batch: AuthoredPoweredCraftBatch,
    batches: u64,
    context: &'static str,
) -> Option<PoweredProjectProjection> {
    let sequence = project_powered_craft_sequence(
        registries,
        state,
        process,
        machine,
        machine_condition,
        drive,
        batch,
        batches,
        context,
    );
    if sequence.batches != batches || sequence.legs.is_empty() {
        return None;
    }
    let store_definition = state
        .energy()
        .get_store(drive)
        .map(|record| record.definition())
        .unwrap_or_else(|| panic!("{context} powered project drive disappeared before planning"));
    let charging = project_manual_power_workload(
        registries,
        MANUAL_POWER_HAND_CRANK,
        EQUIPMENT_STONE_HAND_CRANK,
        crank_condition,
        store_definition,
        sequence.charge_energies(),
        context,
    );
    Some(PoweredProjectProjection { sequence, charging })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredProjectPlan {
    pub(super) process: ProcessId,
    pub(super) source: StockpileId,
    pub(super) destination: StockpileId,
    pub(super) machine: EquipmentId,
    pub(super) crank: EquipmentId,
    pub(super) drive: EnergyStoreId,
    pub(super) batch: AuthoredPoweredCraftBatch,
    pub(super) batches: u64,
    pub(super) context: &'static str,
}

pub(super) fn execute_powered_project(
    registries: &Registries,
    state: &mut AppState,
    plan: PoweredProjectPlan,
) -> PoweredProjectOutcome {
    let PoweredProjectPlan {
        process,
        source,
        destination,
        machine,
        crank,
        drive,
        batch,
        batches,
        context,
    } = plan;
    let mut charge_attention = 0_u64;
    let mut delegated_ticks = 0_u64;
    let mut charge_events = 0_u64;
    let mut maximum_leg_batches = 0_u64;
    let mut completed_batches = 0_u64;
    while completed_batches < batches {
        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(|record| record.stored()),
            Some(Energy::ZERO),
            "{context} packed project must begin each leg with an empty work buffer"
        );
        let machine_record = state
            .equipment()
            .get_equipment(machine)
            .unwrap_or_else(|| panic!("{context} powered machine disappeared before leg planning"));
        let remaining = batches.checked_sub(completed_batches).unwrap_or_else(|| {
            unreachable!("completed powered batches are bounded by the project")
        });
        let sequence = project_powered_craft_sequence(
            registries,
            state,
            process,
            machine_record.definition(),
            machine_record.condition(),
            drive,
            batch,
            remaining,
            context,
        );
        let leg = *sequence
            .legs
            .first()
            .unwrap_or_else(|| panic!("{context} has no feasible powered project leg"));
        let work = validate_start_manual_power(
            registries,
            state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, leg.work),
        )
        .unwrap_or_else(|error| panic!("{context} charge failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("{context} charge commit failed: {error}"));
        charge_attention = charge_attention
            .checked_add(finish_manual_power_work(registries, state, work, context))
            .unwrap_or_else(|| panic!("{context} charge attention overflowed"));
        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(|record| record.stored()),
            Some(leg.work),
            "{context} packed charge must equal the productive leg's exact work"
        );
        let selections = select_stockpile_mass(state, source, leg.input_mass, context);
        let job = validate_start_powered_craft(
            registries,
            state,
            PoweredCraftRequest::new(process, source, selections, machine, drive),
            destination,
        )
        .unwrap_or_else(|error| panic!("{context} powered start failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("{context} powered commit failed: {error}"));
        assert_eq!(
            state.player_work().active(),
            None,
            "{context} powered work must release the player after charging"
        );
        let duration = state
            .production()
            .get_job(job)
            .map(|record| record.active_duration().value())
            .unwrap_or_else(|| panic!("{context} powered job disappeared"));
        delegated_ticks = delegated_ticks
            .checked_add(duration)
            .unwrap_or_else(|| panic!("{context} delegated duration overflowed"));
        finish_uninterrupted_production_job(registries, state, job, context);
        assert_eq!(
            duration, leg.delegated_ticks,
            "{context} executed powered duration diverged from the live packed projection"
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(machine)
                .map(|record| record.condition()),
            Some(leg.condition_after),
            "{context} executed machine wear diverged from the live packed projection"
        );
        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(|record| record.stored()),
            Some(Energy::ZERO),
            "{context} powered leg must consume its complete packed charge"
        );
        completed_batches = completed_batches
            .checked_add(leg.batches)
            .unwrap_or_else(|| panic!("{context} completed batch count overflowed"));
        charge_events = charge_events
            .checked_add(1)
            .unwrap_or_else(|| panic!("{context} charge-event count overflowed"));
        maximum_leg_batches = maximum_leg_batches.max(leg.batches);
    }
    assert_eq!(completed_batches, batches);
    PoweredProjectOutcome {
        charge_attention,
        delegated_ticks,
        charge_events,
        maximum_leg_batches,
    }
}

fn assert_output_equivalence(
    state_a: &AppState,
    stockpile_a: StockpileId,
    state_b: &AppState,
    stockpile_b: StockpileId,
    commodities: &[CommodityKey],
    context: &'static str,
) {
    let a = state_a
        .inventory()
        .get_stockpile(stockpile_a)
        .unwrap_or_else(|| panic!("{context} baseline output disappeared"));
    let b = state_b
        .inventory()
        .get_stockpile(stockpile_b)
        .unwrap_or_else(|| panic!("{context} powered output disappeared"));
    for &commodity in commodities {
        assert_eq!(
            a.get_mass(commodity),
            b.get_mass(commodity),
            "{context} mechanization must preserve the incumbent material transform"
        );
    }
}

#[path = "settlement_workshop_investment/lathe.rs"]
mod lathe;
#[path = "settlement_workshop_investment/toolroom.rs"]
mod toolroom;

#[cfg(not(test))]
pub(super) fn run_lathe_investment_experience(variation_root: Option<u64>) {
    lathe::run_lathe_investment_experience(variation_root);
}

#[cfg(not(test))]
pub(super) fn run_toolroom_investment_experience(variation_root: Option<u64>) {
    toolroom::run_toolroom_investment_experience(variation_root);
}
