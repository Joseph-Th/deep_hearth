//! Pre-action workload planning for primitive and settlement human power.

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_WALKING_WHEEL,
};
use deep_hearth::core::quantity::Energy;
use deep_hearth::core::state::AppState;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{ManualPowerMethodId, ManualPowerProjection, project_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

use super::super::manual_craft_planning::project_manual_assembly_package;
use super::super::seed::mix64;

const MAX_PRIMITIVE_PLANNED_CHARGES: u64 = 160;
const MAX_SETTLEMENT_PLANNED_CHARGES: u64 = 120;

#[derive(Clone, Copy)]
pub(super) struct ShapedBuild {
    pub(super) attention_ticks: u64,
    pub(super) input_mass_mg: u64,
    pub(super) embodied_mass_mg: u64,
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
    pub(super) planned_charges: u64,
    pub(super) crank_build: ShapedBuild,
    pub(super) treadle_build: ShapedBuild,
    pub(super) crank_charge: ManualPowerProjection,
    pub(super) treadle_charge: ManualPowerProjection,
    pub(super) crank_lifecycle_attention: u64,
    pub(super) treadle_lifecycle_attention: u64,
}

#[derive(Clone, Copy)]
pub(super) struct SettlementPowerPlan {
    pub(super) choice: SettlementPowerChoice,
    pub(super) capacity_nj: u128,
    pub(super) planned_charges: u64,
    pub(super) treadle_build: ShapedBuild,
    pub(super) walking_build: ShapedBuild,
    pub(super) treadle_charge: ManualPowerProjection,
    pub(super) walking_charge: ManualPowerProjection,
    pub(super) treadle_lifecycle_attention: u64,
    pub(super) walking_lifecycle_attention: u64,
}

#[derive(Clone, Copy)]
struct ManualPowerRoute {
    method: ManualPowerMethodId,
    equipment: EquipmentDefinitionId,
    store: EnergyStoreDefinitionId,
    requested: Energy,
    context: &'static str,
}

#[derive(Clone, Copy)]
struct ManualPowerLifecycleCost {
    attention_ticks: u64,
    metabolic_nj: u128,
}

impl ManualPowerRoute {
    fn project(self, registries: &Registries, condition: Condition) -> ManualPowerProjection {
        project_manual_power(
            registries,
            self.method,
            self.equipment,
            condition,
            self.store,
            self.requested,
        )
        .unwrap_or_else(|error| {
            panic!(
                "power-provider {} charge projection failed: {error}",
                self.context
            )
        })
    }

    fn project_lifecycle(
        self,
        registries: &Registries,
        planned_charges: u64,
        first_charge: ManualPowerProjection,
    ) -> ManualPowerLifecycleCost {
        assert!(
            planned_charges > 0,
            "power-provider {} lifecycle requires at least one planned charge",
            self.context
        );
        let mut attention_ticks = first_charge.duration().value();
        let mut metabolic_nj = first_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules();
        let mut condition = first_charge.condition_after();

        // Carry the canonical projected condition forward so each later charge pays for the wear
        // created by the earlier projected work instead of extrapolating one pristine-rate sample.
        for _ in 1..planned_charges {
            let charge = self.project(registries, condition);
            attention_ticks = attention_ticks
                .checked_add(charge.duration().value())
                .unwrap_or_else(|| {
                    panic!(
                        "power-provider {} lifecycle attention overflowed",
                        self.context
                    )
                });
            metabolic_nj = metabolic_nj
                .checked_add(charge.resource_budget().metabolic_energy().nanojoules())
                .unwrap_or_else(|| {
                    panic!(
                        "power-provider {} lifecycle metabolism overflowed",
                        self.context
                    )
                });
            condition = charge.condition_after();
        }

        ManualPowerLifecycleCost {
            attention_ticks,
            metabolic_nj,
        }
    }
}

pub(super) fn settlement_power_plan(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    capacity_nj: u128,
    seed: u64,
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
    let treadle_route = ManualPowerRoute {
        method: MANUAL_POWER_FOOT_TREADLE,
        equipment: EQUIPMENT_TIMBER_TREADLE_DRIVE,
        store: ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        requested,
        context: "settlement treadle",
    };
    let walking_route = ManualPowerRoute {
        method: MANUAL_POWER_WALKING_WHEEL,
        equipment: EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        store: ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        requested,
        context: "settlement walking wheel",
    };
    let treadle_charge = treadle_route.project(registries, Condition::PRISTINE);
    let walking_charge = walking_route.project(registries, Condition::PRISTINE);
    let planned_charges = 1 + mix64(seed ^ 0x5345_5454_4C45_5057) % MAX_SETTLEMENT_PLANNED_CHARGES;
    let treadle_lifecycle =
        treadle_route.project_lifecycle(registries, planned_charges, treadle_charge);
    let walking_lifecycle =
        walking_route.project_lifecycle(registries, planned_charges, walking_charge);
    let treadle_lifecycle_attention = treadle_build
        .attention_ticks
        .checked_add(treadle_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("settlement treadle lifecycle attention overflowed"));
    let walking_lifecycle_attention = walking_build
        .attention_ticks
        .checked_add(walking_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("settlement walking-wheel lifecycle attention overflowed"));
    let treadle_key = (
        treadle_lifecycle_attention,
        treadle_lifecycle.metabolic_nj,
        treadle_build.input_mass_mg,
        0_u8,
    );
    let walking_key = (
        walking_lifecycle_attention,
        walking_lifecycle.metabolic_nj,
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
        planned_charges,
        treadle_build,
        walking_build,
        treadle_charge,
        walking_charge,
        treadle_lifecycle_attention,
        walking_lifecycle_attention,
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
    }
}

pub(super) fn primitive_power_plan(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    store_definition: EnergyStoreDefinitionId,
    capacity_nj: u128,
    seed: u64,
) -> PrimitivePowerPlan {
    let crank_build = project_power_package(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_STONE_HAND_CRANK,
        store_definition,
        "power provider crank pre-action build",
    );
    let treadle_build = project_power_package(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        store_definition,
        "power provider treadle pre-action build",
    );
    let requested = Energy::from_nanojoules(capacity_nj);
    let crank_route = ManualPowerRoute {
        method: MANUAL_POWER_HAND_CRANK,
        equipment: EQUIPMENT_STONE_HAND_CRANK,
        store: store_definition,
        requested,
        context: "primitive crank",
    };
    let treadle_route = ManualPowerRoute {
        method: MANUAL_POWER_FOOT_TREADLE,
        equipment: EQUIPMENT_TIMBER_TREADLE_DRIVE,
        store: store_definition,
        requested,
        context: "primitive treadle",
    };
    let crank_charge = crank_route.project(registries, Condition::PRISTINE);
    let treadle_charge = treadle_route.project(registries, Condition::PRISTINE);
    // This is disclosed workload, not a hidden future outcome. The actor knows how many comparable
    // full charges it expects this project to need and invests against that horizon.
    let planned_charges = 1 + mix64(seed ^ 0x504F_5752_574F_524B) % MAX_PRIMITIVE_PLANNED_CHARGES;
    let crank_lifecycle = crank_route.project_lifecycle(registries, planned_charges, crank_charge);
    let treadle_lifecycle =
        treadle_route.project_lifecycle(registries, planned_charges, treadle_charge);
    let crank_lifecycle_attention = crank_build
        .attention_ticks
        .checked_add(crank_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("power provider crank lifecycle attention overflowed"));
    let treadle_lifecycle_attention = treadle_build
        .attention_ticks
        .checked_add(treadle_lifecycle.attention_ticks)
        .unwrap_or_else(|| panic!("power provider treadle lifecycle attention overflowed"));
    let crank_key = (
        crank_lifecycle_attention,
        crank_lifecycle.metabolic_nj,
        crank_build.input_mass_mg,
        0_u8,
    );
    let treadle_key = (
        treadle_lifecycle_attention,
        treadle_lifecycle.metabolic_nj,
        treadle_build.input_mass_mg,
        1_u8,
    );
    PrimitivePowerPlan {
        choice: if crank_key <= treadle_key {
            PrimitivePowerChoice::Crank
        } else {
            PrimitivePowerChoice::Treadle
        },
        store_definition,
        capacity_nj,
        planned_charges,
        crank_build,
        treadle_build,
        crank_charge,
        treadle_charge,
        crank_lifecycle_attention,
        treadle_lifecycle_attention,
    }
}
