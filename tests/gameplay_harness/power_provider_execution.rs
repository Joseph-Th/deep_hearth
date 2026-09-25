//! Canonical matched-arm execution for human-power gameplay evidence.

use std::num::NonZeroU64;

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_WALKING_WHEEL, PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::TickSpan;
use deep_hearth::crafting::{project_manual_craft_hand_work, project_powered_craft_work};
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::equipment::{
    EquipmentId, EquipmentMaintenanceRequest, resolve_equipment_maintenance,
    validate_equipment_maintenance,
};
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{
    ManualPowerMethodId, ManualPowerRequest, project_manual_power, validate_start_manual_power,
};
use deep_hearth::maintenance::MaintenanceBand;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::assess_powered_ore_mass_envelope;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{SurvivalExertion, assess_survival, project_survival_resource_budget};

use super::super::maintenance_timing::finish_active_equipment_maintenance;
use super::super::manual_craft_execution::execute_manual_craft_batches;
use super::super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::super::manual_power_timing::finish_manual_power_work;
use super::build::{build_flywheel, build_provider, stockpile_mass};
use super::consumers::{
    PrimitivePowerConsumer, SettlementPowerConsumer, consume_primitive_charge,
    consume_settlement_charge,
};
use super::planning::{
    PrimitivePowerChoice, PrimitivePowerPlan, SettlementPowerChoice, SettlementPowerPlan,
    ShapedBuild,
};
use super::provisioning::{PowerProjectProvisions, ProvisioningOutcome, provision_for_project_leg};

#[path = "power_provider_execution/project.rs"]
mod project;

use project::{ChargeOutcome, charge_store};
pub(super) use project::{
    ProjectExecutionResources, execute_selected_primitive_project,
    execute_selected_settlement_project,
};

pub(super) struct PrimitiveComparison {
    pub(super) crank_build: ShapedBuild,
    pub(super) crank_drive_build: ShapedBuild,
    pub(super) crank_charge: ChargeOutcome,
    pub(super) treadle_build: ShapedBuild,
    pub(super) treadle_drive_build: ShapedBuild,
    pub(super) treadle_charge: ChargeOutcome,
    pub(super) crank_second_charge: ChargeOutcome,
    pub(super) treadle_second_charge: ChargeOutcome,
    pub(super) crank_consumer_ticks: u64,
    pub(super) treadle_consumer_ticks: u64,
    pub(super) crank_residual_mg: u64,
    pub(super) treadle_residual_mg: u64,
}

pub(super) fn execute_primitive_comparison(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    matter_before: AggregateMass,
    plan: PrimitivePowerPlan,
    consumer: PrimitivePowerConsumer,
) -> PrimitiveComparison {
    let shaped_before_mg = stockpile_mass(state, shaped).milligrams();
    let mut crank_state = state.clone();
    let mut treadle_state = state.clone();
    let (crank, crank_build) = build_provider(
        registries,
        &mut crank_state,
        raw,
        shaped,
        EQUIPMENT_STONE_HAND_CRANK,
        "power provider crank build",
    );
    let (crank_drive, crank_drive_build) = build_flywheel(
        registries,
        &mut crank_state,
        raw,
        shaped,
        plan.store_definition,
        "power provider crank flywheel",
    );
    let crank_charge = charge_store(
        registries,
        &mut crank_state,
        MANUAL_POWER_HAND_CRANK,
        crank,
        crank_drive,
        plan.capacity_nj,
        "power provider crank charge",
    );
    let (treadle, treadle_build) = build_provider(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        "power provider treadle build",
    );
    let (treadle_drive, treadle_drive_build) = build_flywheel(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        plan.store_definition,
        "power provider treadle flywheel",
    );
    let treadle_charge = charge_store(
        registries,
        &mut treadle_state,
        MANUAL_POWER_FOOT_TREADLE,
        treadle,
        treadle_drive,
        plan.capacity_nj,
        "power provider treadle charge",
    );
    let crank_consumer_ticks = consume_primitive_charge(
        registries,
        &mut crank_state,
        consumer,
        crank_drive,
        plan.capacity_nj,
    );
    let treadle_consumer_ticks = consume_primitive_charge(
        registries,
        &mut treadle_state,
        consumer,
        treadle_drive,
        plan.capacity_nj,
    );
    assert_eq!(
        crank_consumer_ticks, treadle_consumer_ticks,
        "matched primitive power providers must feed the same productive consumer duration"
    );
    let crank_second_charge = charge_store(
        registries,
        &mut crank_state,
        MANUAL_POWER_HAND_CRANK,
        crank,
        crank_drive,
        plan.capacity_nj,
        "power provider crank second charge",
    );
    let treadle_second_charge = charge_store(
        registries,
        &mut treadle_state,
        MANUAL_POWER_FOOT_TREADLE,
        treadle,
        treadle_drive,
        plan.capacity_nj,
        "power provider treadle second charge",
    );
    assert_eq!(
        plan.crank_build,
        crank_build.checked_add(crank_drive_build, "crank package"),
        "projected crank package must match executed equipment and store construction"
    );
    assert_eq!(
        plan.treadle_build,
        treadle_build.checked_add(treadle_drive_build, "treadle package"),
        "projected treadle package must match executed equipment and store construction"
    );
    assert_eq!(
        plan.crank_charge.duration().value(),
        crank_charge.attention_ticks
    );
    assert_eq!(
        plan.treadle_charge.duration().value(),
        treadle_charge.attention_ticks
    );
    assert_eq!(
        plan.crank_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        crank_charge.metabolic_nj
    );
    assert_eq!(
        u128::from(
            plan.crank_charge
                .resource_budget()
                .hydration()
                .microliters()
        ),
        crank_charge.hydration_ul
    );
    assert_eq!(
        plan.treadle_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        treadle_charge.metabolic_nj
    );
    assert_eq!(
        u128::from(
            plan.treadle_charge
                .resource_budget()
                .hydration()
                .microliters()
        ),
        treadle_charge.hydration_ul
    );
    assert!(treadle_charge.attention_ticks < crank_charge.attention_ticks);
    for (label, arm) in [("crank", &crank_state), ("treadle", &treadle_state)] {
        assert_eq!(
            calculate_matter_accounting(arm)
                .unwrap_or_else(|error| panic!(
                    "power provider {label} matter audit failed: {error}"
                ))
                .total(),
            matter_before,
            "power provider {label} arm must conserve matter"
        );
        validate_loaded_state(registries, arm)
            .unwrap_or_else(|error| panic!("power provider {label} state invalid: {error}"));
    }
    PrimitiveComparison {
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
        crank_residual_mg: stockpile_mass(&crank_state, shaped)
            .milligrams()
            .checked_sub(shaped_before_mg)
            .unwrap_or_else(|| panic!("crank branch removed shared shaped stock")),
        treadle_residual_mg: stockpile_mass(&treadle_state, shaped)
            .milligrams()
            .checked_sub(shaped_before_mg)
            .unwrap_or_else(|| panic!("treadle branch removed shared shaped stock")),
    }
}

pub(super) struct SettlementComparison {
    pub(super) settlement_treadle_build: ShapedBuild,
    pub(super) settlement_treadle_drive_build: ShapedBuild,
    pub(super) settlement_treadle_charge: ChargeOutcome,
    pub(super) walking_build: ShapedBuild,
    pub(super) walking_drive_build: ShapedBuild,
    pub(super) walking_charge: ChargeOutcome,
    pub(super) settlement_treadle_second_charge: ChargeOutcome,
    pub(super) walking_second_charge: ChargeOutcome,
    pub(super) settlement_treadle_consumer_ticks: u64,
    pub(super) walking_consumer_ticks: u64,
}

pub(super) fn execute_settlement_comparison(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    matter_before: AggregateMass,
    plan: SettlementPowerPlan,
    consumer: SettlementPowerConsumer,
) -> SettlementComparison {
    let mut treadle_state = state.clone();
    let mut walking_state = state.clone();
    let (settlement_treadle, settlement_treadle_build) = build_provider(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        "settlement treadle build",
    );
    let (settlement_treadle_drive, settlement_treadle_drive_build) = build_flywheel(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        "settlement treadle flywheel bank",
    );
    let settlement_treadle_charge = charge_store(
        registries,
        &mut treadle_state,
        MANUAL_POWER_FOOT_TREADLE,
        settlement_treadle,
        settlement_treadle_drive,
        plan.capacity_nj,
        "settlement treadle charge",
    );
    let (walking, walking_build) = build_provider(
        registries,
        &mut walking_state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        "walking-wheel build",
    );
    let (walking_drive, walking_drive_build) = build_flywheel(
        registries,
        &mut walking_state,
        raw,
        shaped,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        "walking-wheel flywheel bank",
    );
    let walking_charge = charge_store(
        registries,
        &mut walking_state,
        MANUAL_POWER_WALKING_WHEEL,
        walking,
        walking_drive,
        plan.capacity_nj,
        "walking-wheel charge",
    );
    let settlement_treadle_consumer_ticks = consume_settlement_charge(
        registries,
        &mut treadle_state,
        consumer,
        settlement_treadle_drive,
        plan.capacity_nj,
    );
    let walking_consumer_ticks = consume_settlement_charge(
        registries,
        &mut walking_state,
        consumer,
        walking_drive,
        plan.capacity_nj,
    );
    assert_eq!(
        settlement_treadle_consumer_ticks, walking_consumer_ticks,
        "matched settlement power providers must feed the same productive consumer duration"
    );
    let settlement_treadle_second_charge = charge_store(
        registries,
        &mut treadle_state,
        MANUAL_POWER_FOOT_TREADLE,
        settlement_treadle,
        settlement_treadle_drive,
        plan.capacity_nj,
        "settlement treadle second charge",
    );
    let walking_second_charge = charge_store(
        registries,
        &mut walking_state,
        MANUAL_POWER_WALKING_WHEEL,
        walking,
        walking_drive,
        plan.capacity_nj,
        "walking-wheel second charge",
    );
    assert_eq!(
        plan.treadle_build,
        settlement_treadle_build
            .checked_add(settlement_treadle_drive_build, "settlement treadle package",),
        "projected settlement treadle package must match executed construction"
    );
    assert_eq!(
        plan.walking_build,
        walking_build.checked_add(walking_drive_build, "walking-wheel package"),
        "projected walking-wheel package must match executed construction"
    );
    assert_eq!(
        plan.treadle_charge.duration().value(),
        settlement_treadle_charge.attention_ticks
    );
    assert_eq!(
        plan.walking_charge.duration().value(),
        walking_charge.attention_ticks
    );
    assert_eq!(
        plan.treadle_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        settlement_treadle_charge.metabolic_nj
    );
    assert_eq!(
        plan.walking_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        walking_charge.metabolic_nj
    );
    assert!(walking_charge.attention_ticks < settlement_treadle_charge.attention_ticks);
    assert!(walking_charge.metabolic_nj < settlement_treadle_charge.metabolic_nj);
    for (label, arm) in [
        ("settlement treadle", &treadle_state),
        ("walking-wheel", &walking_state),
    ] {
        assert_eq!(
            calculate_matter_accounting(arm)
                .unwrap_or_else(|error| panic!("{label} matter audit failed: {error}"))
                .total(),
            matter_before,
            "{label} arm must conserve matter"
        );
        validate_loaded_state(registries, arm)
            .unwrap_or_else(|error| panic!("{label} state invalid: {error}"));
    }
    SettlementComparison {
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
    }
}
