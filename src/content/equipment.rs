//! Built-in workshop equipment definitions.

use crate::capability::{CapabilityProfile, CapabilityValue};
use crate::core::quantity::{Mass, MassFlow, Power, Temperature};
use crate::equipment::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    EquipmentMaintenanceProfile, EquipmentRegistry,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::CommodityKey;

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW,
    CAPABILITY_GRINDER_BATCH, CAPABILITY_GRINDER_FLOW, CAPABILITY_HEATING_POWER,
    CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW, CAPABILITY_THERMAL_BATCH,
    CAPABILITY_THERMAL_MAX_TEMPERATURE,
};
use super::materials::{FORM_INGOT, MATERIAL_COPPER};

pub const EQUIPMENT_JAW_CRUSHER: EquipmentDefinitionId = EquipmentDefinitionId::new(1);
pub const EQUIPMENT_ELECTRIC_FURNACE: EquipmentDefinitionId = EquipmentDefinitionId::new(2);
pub const EQUIPMENT_CASTING_MOLD: EquipmentDefinitionId = EquipmentDefinitionId::new(3);
pub const EQUIPMENT_DRY_SCREEN: EquipmentDefinitionId = EquipmentDefinitionId::new(4);
pub const EQUIPMENT_GRINDING_MILL: EquipmentDefinitionId = EquipmentDefinitionId::new(5);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("built-in equipment condition is invalid: {error}"),
    }
}

fn crusher_maintenance() -> EquipmentMaintenanceProfile {
    EquipmentMaintenanceProfile::new(
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        Mass::from_milligrams(50_000),
        condition(900_000),
    )
}

fn thresholds() -> MaintenanceThresholds {
    match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("built-in equipment maintenance thresholds are invalid: {error}"),
    }
}

fn profile(
    entries: impl IntoIterator<Item = (crate::capability::CapabilityId, CapabilityValue)>,
) -> CapabilityProfile {
    match CapabilityProfile::new(entries) {
        Ok(profile) => profile,
        Err(error) => panic!("built-in equipment capability profile is invalid: {error}"),
    }
}

pub(crate) fn build_equipment_registry() -> EquipmentRegistry {
    let crusher_curve = CapabilityConditionCurve::new(
        CAPABILITY_CRUSHER_FLOW,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::MassFlow(MassFlow::ZERO),
            ),
            CapabilityConditionPoint::new(
                condition(600_000),
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(20)),
            ),
        ],
    );
    let grinder_curve = CapabilityConditionCurve::new(
        CAPABILITY_GRINDER_FLOW,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::MassFlow(MassFlow::ZERO),
            ),
            CapabilityConditionPoint::new(
                condition(600_000),
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(10)),
            ),
        ],
    );
    let screen_curve = CapabilityConditionCurve::new(
        CAPABILITY_SCREEN_FLOW,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::MassFlow(MassFlow::ZERO),
            ),
            CapabilityConditionPoint::new(
                condition(600_000),
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(40)),
            ),
        ],
    );
    EquipmentRegistry::new([
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_JAW_CRUSHER,
            "workshop jaw crusher",
            Mass::from_milligrams(3_600_000_000),
            profile([
                (
                    CAPABILITY_CRUSHER_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(40)),
                ),
                (
                    CAPABILITY_CRUSHER_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20)),
                ),
            ]),
            thresholds(),
            vec![crusher_curve],
        )
        .with_maintenance_profile(crusher_maintenance()),
        EquipmentDefinition::new(
            EQUIPMENT_ELECTRIC_FURNACE,
            "workshop electric furnace",
            Mass::from_milligrams(500_000_000),
            profile([
                (
                    CAPABILITY_HEATING_POWER,
                    CapabilityValue::Power(Power::from_microwatts(20_000_000)),
                ),
                (
                    CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
                ),
                (
                    CAPABILITY_THERMAL_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20)),
                ),
            ]),
            thresholds(),
        ),
        EquipmentDefinition::new(
            EQUIPMENT_CASTING_MOLD,
            "workshop cooled casting mold",
            Mass::from_milligrams(100_000_000),
            profile([
                (
                    CAPABILITY_COOLING_POWER,
                    CapabilityValue::Power(Power::from_microwatts(10_000_000)),
                ),
                (
                    CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_600_000)),
                ),
                (
                    CAPABILITY_THERMAL_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20)),
                ),
            ]),
            thresholds(),
        ),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_DRY_SCREEN,
            "workshop dry screen",
            Mass::from_milligrams(1_200_000_000),
            profile([
                (
                    CAPABILITY_SCREEN_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(80)),
                ),
                (
                    CAPABILITY_SCREEN_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20)),
                ),
            ]),
            thresholds(),
            vec![screen_curve],
        ),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_GRINDING_MILL,
            "workshop grinding mill",
            Mass::from_milligrams(2_400_000_000),
            profile([
                (
                    CAPABILITY_GRINDER_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(20)),
                ),
                (
                    CAPABILITY_GRINDER_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20)),
                ),
            ]),
            thresholds(),
            vec![grinder_curve],
        ),
    ])
}
