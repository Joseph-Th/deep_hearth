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
use deep_hearth::labor::{ManualPowerProjection, project_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

use super::super::manual_craft_planning::project_manual_assembly_package;
use super::super::seed::mix64;

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
    let treadle_charge = project_manual_power(
        registries,
        MANUAL_POWER_FOOT_TREADLE,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        Condition::PRISTINE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        requested,
    )
    .unwrap_or_else(|error| panic!("settlement treadle pre-action charge failed: {error}"));
    let walking_charge = project_manual_power(
        registries,
        MANUAL_POWER_WALKING_WHEEL,
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        Condition::PRISTINE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        requested,
    )
    .unwrap_or_else(|error| panic!("walking-wheel pre-action charge failed: {error}"));
    let planned_charges = 1 + mix64(seed ^ 0x5345_5454_4C45_5057) % 120;
    let lifecycle_attention = |build: ShapedBuild, charge: ManualPowerProjection| {
        build
            .attention_ticks
            .checked_add(
                charge
                    .duration()
                    .value()
                    .checked_mul(planned_charges)
                    .unwrap_or_else(|| panic!("settlement charge horizon overflowed")),
            )
            .unwrap_or_else(|| panic!("settlement lifecycle attention overflowed"))
    };
    let lifecycle_metabolic = |charge: ManualPowerProjection| {
        charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules()
            .checked_mul(u128::from(planned_charges))
            .unwrap_or_else(|| panic!("settlement metabolic horizon overflowed"))
    };
    let treadle_lifecycle_attention = lifecycle_attention(treadle_build, treadle_charge);
    let walking_lifecycle_attention = lifecycle_attention(walking_build, walking_charge);
    let treadle_key = (
        treadle_lifecycle_attention,
        lifecycle_metabolic(treadle_charge),
        treadle_build.input_mass_mg,
        0_u8,
    );
    let walking_key = (
        walking_lifecycle_attention,
        lifecycle_metabolic(walking_charge),
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
    let crank_charge = project_manual_power(
        registries,
        MANUAL_POWER_HAND_CRANK,
        EQUIPMENT_STONE_HAND_CRANK,
        Condition::PRISTINE,
        store_definition,
        requested,
    )
    .unwrap_or_else(|error| panic!("power provider crank pre-action charge failed: {error}"));
    let treadle_charge = project_manual_power(
        registries,
        MANUAL_POWER_FOOT_TREADLE,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        Condition::PRISTINE,
        store_definition,
        requested,
    )
    .unwrap_or_else(|error| panic!("power provider treadle pre-action charge failed: {error}"));
    // This is disclosed workload, not a hidden future outcome. The actor knows how many comparable
    // full charges it expects this project to need and invests against that horizon.
    let planned_charges = 1 + mix64(seed ^ 0x504F_5752_574F_524B) % 160;
    let lifecycle_attention = |build: ShapedBuild, charge: ManualPowerProjection| {
        build
            .attention_ticks
            .checked_add(
                charge
                    .duration()
                    .value()
                    .checked_mul(planned_charges)
                    .unwrap_or_else(|| {
                        panic!("power provider projected charge horizon overflowed")
                    }),
            )
            .unwrap_or_else(|| panic!("power provider projected lifecycle attention overflowed"))
    };
    let crank_lifecycle_attention = lifecycle_attention(crank_build, crank_charge);
    let treadle_lifecycle_attention = lifecycle_attention(treadle_build, treadle_charge);
    let lifecycle_metabolic = |charge: ManualPowerProjection| {
        charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules()
            .checked_mul(u128::from(planned_charges))
            .unwrap_or_else(|| panic!("power provider projected metabolic horizon overflowed"))
    };
    let crank_key = (
        crank_lifecycle_attention,
        lifecycle_metabolic(crank_charge),
        crank_build.input_mass_mg,
        0_u8,
    );
    let treadle_key = (
        treadle_lifecycle_attention,
        lifecycle_metabolic(treadle_charge),
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
