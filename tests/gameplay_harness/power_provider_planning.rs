//! Pre-action workload planning for primitive and settlement human power.

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_WALKING_WHEEL, PROCESS_CRUSH_ORE,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::ManualPowerProjection;
use deep_hearth::maintenance::Condition;
use deep_hearth::ore_processing::{
    PoweredOreOrderMaintenancePolicy, PoweredOreOrderRequest, project_powered_ore_order,
};
use deep_hearth::registry::Registries;

use super::super::manual_craft_planning::project_manual_assembly_package;

#[path = "power_provider_policy.rs"]
mod policy;
use policy::{
    primitive_treadle_clears_attention_return, primitive_treadle_minimum_attention_return,
};

#[path = "power_provider_planning/lifecycle.rs"]
mod lifecycle;
use lifecycle::{
    ManualPowerRoute, charge_events_for_declared_work, first_candidate_preferred_charge,
};

const MAX_PRIMITIVE_CROSSOVER_CHARGES: u64 = 512;
const MAX_PRIMITIVE_PROJECT_BATCHES: u64 = 1_024;
const MAX_SETTLEMENT_CROSSOVER_CHARGES: u64 = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ShapedBuild {
    pub(super) attention_ticks: u64,
    pub(super) input_mass_mg: u64,
    pub(super) embodied_mass_mg: u64,
    pub(super) metabolic_nj: u128,
    pub(super) hydration_ul: u64,
}

impl ShapedBuild {
    pub(super) fn checked_add(self, other: Self, context: &'static str) -> Self {
        Self {
            attention_ticks: self
                .attention_ticks
                .checked_add(other.attention_ticks)
                .unwrap_or_else(|| panic!("{context} attention overflowed")),
            input_mass_mg: self
                .input_mass_mg
                .checked_add(other.input_mass_mg)
                .unwrap_or_else(|| panic!("{context} input mass overflowed")),
            embodied_mass_mg: self
                .embodied_mass_mg
                .checked_add(other.embodied_mass_mg)
                .unwrap_or_else(|| panic!("{context} embodied mass overflowed")),
            metabolic_nj: self
                .metabolic_nj
                .checked_add(other.metabolic_nj)
                .unwrap_or_else(|| panic!("{context} metabolism overflowed")),
            hydration_ul: self
                .hydration_ul
                .checked_add(other.hydration_ul)
                .unwrap_or_else(|| panic!("{context} hydration overflowed")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrimitivePowerChoice {
    Crank,
    Treadle,
}

impl PrimitivePowerChoice {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Crank => "crank",
            Self::Treadle => "treadle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettlementPowerChoice {
    Treadle,
    WalkingWheel,
}

impl SettlementPowerChoice {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Treadle => "treadle",
            Self::WalkingWheel => "walking-wheel",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct PrimitivePowerPlan {
    pub(super) choice: PrimitivePowerChoice,
    pub(super) store_definition: EnergyStoreDefinitionId,
    pub(super) capacity_nj: u128,
    pub(super) declared_work_nj: u128,
    pub(super) charge_events: u64,
    pub(super) consumer_projected_charge_events: u64,
    pub(super) consumer_projected_services: u64,
    pub(super) crank_build: ShapedBuild,
    pub(super) treadle_build: ShapedBuild,
    pub(super) crank_charge: ManualPowerProjection,
    pub(super) treadle_charge: ManualPowerProjection,
    pub(super) crank_lifecycle_attention: u64,
    pub(super) treadle_lifecycle_attention: u64,
    pub(super) crank_lifecycle_metabolic_nj: u128,
    pub(super) treadle_lifecycle_metabolic_nj: u128,
    pub(super) crank_lifecycle_hydration_ul: u64,
    pub(super) treadle_lifecycle_hydration_ul: u64,
    pub(super) crank_lifecycle_condition: Condition,
    pub(super) treadle_lifecycle_condition: Condition,
    pub(super) minimum_attention_return_ticks: u64,
    pub(super) decision_crossover_charges: Option<u64>,
}

#[derive(Clone, Copy)]
pub(super) struct PrimitivePowerProject {
    pub(super) store_definition: EnergyStoreDefinitionId,
    pub(super) capacity_nj: u128,
    pub(super) consumer: EquipmentId,
    pub(super) declared_mass: Mass,
    pub(super) declared_work_nj: u128,
}

#[derive(Clone, Copy)]
pub(super) struct SettlementPowerPlan {
    pub(super) choice: SettlementPowerChoice,
    pub(super) capacity_nj: u128,
    pub(super) declared_work_nj: u128,
    pub(super) charge_events: u64,
    pub(super) treadle_build: ShapedBuild,
    pub(super) walking_build: ShapedBuild,
    pub(super) treadle_charge: ManualPowerProjection,
    pub(super) walking_charge: ManualPowerProjection,
    pub(super) treadle_lifecycle_attention: u64,
    pub(super) walking_lifecycle_attention: u64,
    pub(super) treadle_lifecycle_metabolic_nj: u128,
    pub(super) walking_lifecycle_metabolic_nj: u128,
    pub(super) treadle_lifecycle_hydration_ul: u64,
    pub(super) walking_lifecycle_hydration_ul: u64,
    pub(super) treadle_lifecycle_condition: Condition,
    pub(super) walking_lifecycle_condition: Condition,
    pub(super) decision_crossover_charges: Option<u64>,
}

pub(super) fn settlement_power_plan(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    capacity_nj: u128,
    declared_work_nj: u128,
) -> SettlementPowerPlan {
    let treadle_build = project_power_package(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        "settlement treadle pre-action build",
    );
    let walking_build = project_power_package(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        "walking-wheel pre-action build",
    );
    let requested = Energy::from_nanojoules(capacity_nj);
    let treadle_route = ManualPowerRoute::new(
        MANUAL_POWER_FOOT_TREADLE,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        requested,
        "settlement treadle",
    );
    let walking_route = ManualPowerRoute::new(
        MANUAL_POWER_WALKING_WHEEL,
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        requested,
        "settlement walking wheel",
    );
    let decision_crossover_charges = first_candidate_preferred_charge(
        registries,
        treadle_route,
        treadle_build,
        walking_route,
        walking_build,
        MAX_SETTLEMENT_CROSSOVER_CHARGES,
        0,
    );
    let treadle_charge = treadle_route.project(registries, Condition::PRISTINE);
    let walking_charge = walking_route.project(registries, Condition::PRISTINE);
    let charge_events =
        charge_events_for_declared_work(declared_work_nj, capacity_nj, "settlement project");
    let declared_work = Energy::from_nanojoules(declared_work_nj);
    let treadle_lifecycle = treadle_route.project_lifecycle(registries, declared_work);
    let walking_lifecycle = walking_route.project_lifecycle(registries, declared_work);
    let treadle_lifecycle_attention = treadle_build
        .attention_ticks
        .checked_add(treadle_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("settlement treadle lifecycle attention overflowed"));
    let walking_lifecycle_attention = walking_build
        .attention_ticks
        .checked_add(walking_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("settlement walking-wheel lifecycle attention overflowed"));
    let treadle_lifecycle_metabolic_nj = treadle_build
        .metabolic_nj
        .checked_add(treadle_lifecycle.metabolic_nj)
        .unwrap_or_else(|| panic!("settlement treadle total metabolism overflowed"));
    let walking_lifecycle_metabolic_nj = walking_build
        .metabolic_nj
        .checked_add(walking_lifecycle.metabolic_nj)
        .unwrap_or_else(|| panic!("settlement walking total metabolism overflowed"));
    let treadle_lifecycle_hydration_ul = treadle_build
        .hydration_ul
        .checked_add(treadle_lifecycle.hydration_ul)
        .unwrap_or_else(|| panic!("settlement treadle total hydration overflowed"));
    let walking_lifecycle_hydration_ul = walking_build
        .hydration_ul
        .checked_add(walking_lifecycle.hydration_ul)
        .unwrap_or_else(|| panic!("settlement walking total hydration overflowed"));
    let treadle_key = (
        treadle_lifecycle_attention,
        treadle_lifecycle_metabolic_nj,
        treadle_lifecycle_hydration_ul,
        treadle_build.input_mass_mg,
        0_u8,
    );
    let walking_key = (
        walking_lifecycle_attention,
        walking_lifecycle_metabolic_nj,
        walking_lifecycle_hydration_ul,
        walking_build.input_mass_mg,
        1_u8,
    );
    SettlementPowerPlan {
        choice: if treadle_key <= walking_key {
            SettlementPowerChoice::Treadle
        } else {
            SettlementPowerChoice::WalkingWheel
        },
        capacity_nj,
        declared_work_nj,
        charge_events,
        treadle_build,
        walking_build,
        treadle_charge,
        walking_charge,
        treadle_lifecycle_attention,
        walking_lifecycle_attention,
        treadle_lifecycle_metabolic_nj,
        walking_lifecycle_metabolic_nj,
        treadle_lifecycle_hydration_ul,
        walking_lifecycle_hydration_ul,
        treadle_lifecycle_condition: treadle_lifecycle.condition_after,
        walking_lifecycle_condition: walking_lifecycle.condition_after,
        decision_crossover_charges,
    }
}

fn project_power_package(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    equipment: EquipmentDefinitionId,
    store: EnergyStoreDefinitionId,
    context: &'static str,
) -> ShapedBuild {
    let equipment_profile = registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|equipment| equipment.assembly_profile())
        .unwrap_or_else(|| {
            panic!(
                "power provider equipment {} lost authored assembly",
                equipment.value()
            )
        });
    let store_profile = registries
        .energy()
        .get_store(store)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| {
            panic!(
                "power provider store {} lost authored assembly",
                store.value()
            )
        });
    let projection = project_manual_assembly_package(
        registries,
        state,
        &[raw],
        shaped,
        &[equipment_profile, store_profile],
        context,
    );
    ShapedBuild {
        attention_ticks: projection.attention_ticks,
        input_mass_mg: projection.input_mass_mg,
        embodied_mass_mg: projection.embodied_mass_mg,
        metabolic_nj: projection.metabolic_nj,
        hydration_ul: projection.hydration_ul,
    }
}

pub(super) fn primitive_power_plan(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    project: PrimitivePowerProject,
) -> PrimitivePowerPlan {
    let crank_build = project_power_package(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_STONE_HAND_CRANK,
        project.store_definition,
        "power provider crank pre-action build",
    );
    let treadle_build = project_power_package(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        project.store_definition,
        "power provider treadle pre-action build",
    );
    let requested = Energy::from_nanojoules(project.capacity_nj);
    let crank_route = ManualPowerRoute::new(
        MANUAL_POWER_HAND_CRANK,
        EQUIPMENT_STONE_HAND_CRANK,
        project.store_definition,
        requested,
        "primitive crank",
    );
    let treadle_route = ManualPowerRoute::new(
        MANUAL_POWER_FOOT_TREADLE,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        project.store_definition,
        requested,
        "primitive treadle",
    );
    let crank_charge = crank_route.project(registries, Condition::PRISTINE);
    let treadle_charge = treadle_route.project(registries, Condition::PRISTINE);
    // The project owns a fixed amount of useful mechanical work. Buffer choice only determines
    // how many charging events are needed; it cannot silently resize the player's project.
    let charge_events = charge_events_for_declared_work(
        project.declared_work_nj,
        project.capacity_nj,
        "primitive project",
    );
    let consumer_record = state
        .equipment()
        .get_equipment(project.consumer)
        .unwrap_or_else(|| panic!("power-provider primitive consumer disappeared before planning"));
    let consumer_order = project_powered_ore_order(
        registries,
        PROCESS_CRUSH_ORE,
        consumer_record.definition(),
        project.store_definition,
        PoweredOreOrderRequest::new(
            consumer_record.condition(),
            project.declared_mass,
            MAX_PRIMITIVE_PROJECT_BATCHES,
            PoweredOreOrderMaintenancePolicy::ServiceAtCritical,
        ),
    )
    .unwrap_or_else(|error| {
        panic!("power-provider primitive consumer order projection failed: {error}")
    });
    let consumer_projected_charge_events = u64::try_from(consumer_order.batches().len())
        .unwrap_or_else(|_| panic!("power-provider projected charge count exceeds u64"));
    assert!(
        consumer_projected_charge_events >= charge_events,
        "consumer wear cannot reduce the buffer-only charge lower bound"
    );
    let projected_work_nj = consumer_order
        .batches()
        .iter()
        .try_fold(0_u128, |total, batch| {
            total.checked_add(batch.required_energy().nanojoules())
        })
        .unwrap_or_else(|| panic!("power-provider projected consumer work overflowed"));
    assert_eq!(
        projected_work_nj, project.declared_work_nj,
        "consumer-aware batch projection must preserve the declared useful work"
    );
    let crank_lifecycle =
        crank_route.project_lifecycle_batches(registries, consumer_order.batches());
    let treadle_lifecycle =
        treadle_route.project_lifecycle_batches(registries, consumer_order.batches());
    let minimum_attention_return_ticks = primitive_treadle_minimum_attention_return(
        crank_build.attention_ticks,
        treadle_build.attention_ticks,
    );
    let decision_crossover_charges = first_candidate_preferred_charge(
        registries,
        crank_route,
        crank_build,
        treadle_route,
        treadle_build,
        MAX_PRIMITIVE_CROSSOVER_CHARGES,
        minimum_attention_return_ticks,
    );
    let crank_lifecycle_attention = crank_build
        .attention_ticks
        .checked_add(crank_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("power provider crank lifecycle attention overflowed"));
    let treadle_lifecycle_attention = treadle_build
        .attention_ticks
        .checked_add(treadle_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("power provider treadle lifecycle attention overflowed"));
    let crank_lifecycle_metabolic_nj = crank_build
        .metabolic_nj
        .checked_add(crank_lifecycle.metabolic_nj)
        .unwrap_or_else(|| panic!("power provider crank total metabolism overflowed"));
    let treadle_lifecycle_metabolic_nj = treadle_build
        .metabolic_nj
        .checked_add(treadle_lifecycle.metabolic_nj)
        .unwrap_or_else(|| panic!("power provider treadle total metabolism overflowed"));
    let crank_lifecycle_hydration_ul = crank_build
        .hydration_ul
        .checked_add(crank_lifecycle.hydration_ul)
        .unwrap_or_else(|| panic!("power provider crank total hydration overflowed"));
    let treadle_lifecycle_hydration_ul = treadle_build
        .hydration_ul
        .checked_add(treadle_lifecycle.hydration_ul)
        .unwrap_or_else(|| panic!("power provider treadle total hydration overflowed"));
    PrimitivePowerPlan {
        choice: if primitive_treadle_clears_attention_return(
            crank_lifecycle_attention,
            treadle_lifecycle_attention,
            minimum_attention_return_ticks,
        ) {
            PrimitivePowerChoice::Treadle
        } else {
            PrimitivePowerChoice::Crank
        },
        store_definition: project.store_definition,
        capacity_nj: project.capacity_nj,
        declared_work_nj: project.declared_work_nj,
        charge_events,
        consumer_projected_charge_events,
        consumer_projected_services: consumer_order.services(),
        crank_build,
        treadle_build,
        crank_charge,
        treadle_charge,
        crank_lifecycle_attention,
        treadle_lifecycle_attention,
        crank_lifecycle_metabolic_nj,
        treadle_lifecycle_metabolic_nj,
        crank_lifecycle_hydration_ul,
        treadle_lifecycle_hydration_ul,
        crank_lifecycle_condition: crank_lifecycle.condition_after,
        treadle_lifecycle_condition: treadle_lifecycle.condition_after,
        minimum_attention_return_ticks,
        decision_crossover_charges,
    }
}
