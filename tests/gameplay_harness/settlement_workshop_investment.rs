//! Full capital-crossover experiences for later settlement workshop machines.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_SPRING_POLE_LATHE, EQUIPMENT_TIMBER_TREADLE_GRINDSTONE, FORM_BOARD, FORM_CHIP,
    FORM_FLYWHEEL, FORM_GRINDSTONE_WHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL,
    FORM_SCRAP, FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_GRIND_STONE_SCRAP_TOOL,
    PROCESS_POWER_GRIND_STONE_SCRAP_TOOL, PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
    PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_TIMBER_FLYWHEEL, PROCESS_SHAPE_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    ManualCraftRequest, PoweredCraftRequest, resolve_manual_craft, validate_start_powered_craft,
};
use deep_hearth::energy::{EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{
    EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile,
};
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::environment::ROOM_TEMPERATURE;
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::powered_craft_planning::{AuthoredPoweredCraftBatch, authored_batch};
use super::production_timing::finish_uninterrupted_production_job;
use super::settlement_power_planning::{ManualPowerSequenceRequest, project_manual_power_sequence};

const SHORT_LATHE_ORDER: u64 = 8;
const PROJECT_LATHE_ORDER: u64 = 14;
const SHORT_TOOLROOM_ORDER: u64 = 8;
const PROJECT_TOOLROOM_ORDER: u64 = 16;

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

fn seed_common_upgrade_raw(
    registries: &Registries,
    state: &mut AppState,
) -> (StockpileId, StockpileId) {
    let raw = seed_stockpile(
        state,
        Mass::from_milligrams(5_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(4_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            Mass::from_milligrams(20_000),
        ),
    ] {
        seed_material(registries, state, raw, commodity, mass);
    }
    let parts = seed_stockpile(
        state,
        Mass::from_milligrams(5_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    (raw, parts)
}

fn setup_steps(
    handle_equipment: Option<EquipmentId>,
) -> [(ProcessId, u64, Option<EquipmentId>, &'static str); 4] {
    [
        (
            PROCESS_SHAPE_STONE_FLYWHEEL,
            1,
            None,
            "settlement flywheel-upgrade stone flywheel",
        ),
        (
            PROCESS_SHAPE_WOOD_BOARDS,
            2,
            None,
            "settlement flywheel-upgrade boards",
        ),
        (
            PROCESS_SHAPE_WOOD_HANDLE,
            2,
            handle_equipment,
            "settlement flywheel-upgrade handles",
        ),
        (
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            1,
            None,
            "settlement flywheel-upgrade bearing reinforcement",
        ),
    ]
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
    handle_equipment: Option<EquipmentId>,
) -> u64 {
    setup_steps(handle_equipment)
        .into_iter()
        .map(|(process, batches, equipment, context)| {
            let request =
                setup_request(registries, state, raw, process, batches, equipment, context);
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
    handle_equipment: Option<EquipmentId>,
) -> u64 {
    setup_steps(handle_equipment)
        .into_iter()
        .map(|(process, batches, equipment, context)| {
            let request =
                setup_request(registries, state, raw, process, batches, equipment, context);
            execute_manual_craft(registries, state, request, parts, context).value()
        })
        .try_fold(0_u64, u64::checked_add)
        .unwrap_or_else(|| panic!("settlement flywheel-upgrade execution attention overflowed"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PoweredProjectOutcome {
    charge_attention: u64,
    delegated_ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PoweredProjectPlan {
    process: ProcessId,
    source: StockpileId,
    input_lot: MaterialLotId,
    destination: StockpileId,
    machine: EquipmentId,
    crank: EquipmentId,
    drive: EnergyStoreId,
    batch: AuthoredPoweredCraftBatch,
    batches: u64,
    context: &'static str,
}

fn execute_powered_project(
    registries: &Registries,
    state: &mut AppState,
    plan: PoweredProjectPlan,
) -> PoweredProjectOutcome {
    let PoweredProjectPlan {
        process,
        source,
        input_lot,
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
    for _ in 0..batches {
        let work = validate_start_manual_power(
            registries,
            state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, batch.work),
        )
        .unwrap_or_else(|error| panic!("{context} charge failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("{context} charge commit failed: {error}"));
        charge_attention = charge_attention
            .checked_add(finish_manual_power_work(registries, state, work, context))
            .unwrap_or_else(|| panic!("{context} charge attention overflowed"));
        let job = validate_start_powered_craft(
            registries,
            state,
            PoweredCraftRequest::single(
                process,
                source,
                MaterialLotSelection::new(input_lot, batch.input_mass),
                machine,
                drive,
            ),
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
    }
    PoweredProjectOutcome {
        charge_attention,
        delegated_ticks,
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
pub(super) fn run_lathe_investment_experience() {
    lathe::run_lathe_investment_experience();
}

#[cfg(not(test))]
pub(super) fn run_toolroom_investment_experience() {
    toolroom::run_toolroom_investment_experience();
}
