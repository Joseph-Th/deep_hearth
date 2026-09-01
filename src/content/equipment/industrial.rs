//! Fixed workshop machinery definitions.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow, Power, Temperature};
use crate::equipment::EquipmentDefinition;

use super::authoring::{
    industrial_maintenance, mass_flow_condition_curve, power_condition_curve, profile, thresholds,
};
use super::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE,
    EQUIPMENT_GRAVITY_SEPARATOR, EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER,
};
use crate::content::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW,
    CAPABILITY_GRINDER_BATCH, CAPABILITY_GRINDER_FLOW, CAPABILITY_HEATING_POWER,
    CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW, CAPABILITY_SEPARATOR_BATCH,
    CAPABILITY_SEPARATOR_FLOW, CAPABILITY_THERMAL_BATCH, CAPABILITY_THERMAL_MAX_TEMPERATURE,
};

const JAW_CRUSHER_MASS: Mass = Mass::from_milligrams(3_600_000_000);
const ELECTRIC_FURNACE_MASS: Mass = Mass::from_milligrams(500_000_000);
const CASTING_MOLD_MASS: Mass = Mass::from_milligrams(100_000_000);
const DRY_SCREEN_MASS: Mass = Mass::from_milligrams(1_200_000_000);
const GRINDING_MILL_MASS: Mass = Mass::from_milligrams(2_400_000_000);
const GRAVITY_SEPARATOR_MASS: Mass = Mass::from_milligrams(1_600_000_000);

pub(super) fn definitions() -> [EquipmentDefinition; 6] {
    [
        jaw_crusher(),
        electric_furnace(),
        casting_mold(),
        dry_screen(),
        grinding_mill(),
        gravity_separator(),
    ]
}

fn jaw_crusher() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_JAW_CRUSHER,
        "workshop jaw crusher",
        JAW_CRUSHER_MASS,
        profile([
            (
                CAPABILITY_CRUSHER_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(4_000_000)),
            ),
            (
                CAPABILITY_CRUSHER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_CRUSHER_FLOW,
            600_000,
            MassFlow::from_milligrams_per_second(2_000_000),
        )],
    )
    .with_required_structural_support()
    .with_maintenance_profile(industrial_maintenance(JAW_CRUSHER_MASS))
}

fn electric_furnace() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_ELECTRIC_FURNACE,
        "workshop electric furnace",
        ELECTRIC_FURNACE_MASS,
        profile([
            (
                CAPABILITY_HEATING_POWER,
                CapabilityValue::Power(Power::from_microwatts(2_000_000_000_000)),
            ),
            (
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
            ),
            (
                CAPABILITY_THERMAL_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
            ),
        ]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_HEATING_POWER,
            600_000,
            Power::from_microwatts(1_000_000_000_000),
        )],
    )
    .with_required_structural_support()
    .with_maintenance_profile(industrial_maintenance(ELECTRIC_FURNACE_MASS))
}

fn casting_mold() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_CASTING_MOLD,
        "workshop cooled casting mold",
        CASTING_MOLD_MASS,
        profile([
            (
                CAPABILITY_COOLING_POWER,
                CapabilityValue::Power(Power::from_microwatts(1_000_000_000_000)),
            ),
            (
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_600_000)),
            ),
            (
                CAPABILITY_THERMAL_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
            ),
        ]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_COOLING_POWER,
            600_000,
            Power::from_microwatts(500_000_000_000),
        )],
    )
    .with_required_structural_support()
    .with_maintenance_profile(industrial_maintenance(CASTING_MOLD_MASS))
}

fn dry_screen() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_DRY_SCREEN,
        "workshop dry screen",
        DRY_SCREEN_MASS,
        profile([
            (
                CAPABILITY_SCREEN_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(8_000_000)),
            ),
            (
                CAPABILITY_SCREEN_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_SCREEN_FLOW,
            600_000,
            MassFlow::from_milligrams_per_second(4_000_000),
        )],
    )
    .with_required_structural_support()
    .with_maintenance_profile(industrial_maintenance(DRY_SCREEN_MASS))
}

fn grinding_mill() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_GRINDING_MILL,
        "workshop grinding mill",
        GRINDING_MILL_MASS,
        profile([
            (
                CAPABILITY_GRINDER_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(2_000_000)),
            ),
            (
                CAPABILITY_GRINDER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_GRINDER_FLOW,
            600_000,
            MassFlow::from_milligrams_per_second(1_000_000),
        )],
    )
    .with_required_structural_support()
    .with_maintenance_profile(industrial_maintenance(GRINDING_MILL_MASS))
}

fn gravity_separator() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_GRAVITY_SEPARATOR,
        "workshop gravity separator",
        GRAVITY_SEPARATOR_MASS,
        profile([
            (
                CAPABILITY_SEPARATOR_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(500_000)),
            ),
            (
                CAPABILITY_SEPARATOR_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_SEPARATOR_FLOW,
            600_000,
            MassFlow::from_milligrams_per_second(250_000),
        )],
    )
    .with_required_structural_support()
    .with_maintenance_profile(industrial_maintenance(GRAVITY_SEPARATOR_MASS))
}
