//! First ordinary copper foundry equipment built from the primitive workshop material vocabulary.

use crate::capability::CapabilityValue;
use crate::content::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_HEATING_POWER, CAPABILITY_THERMAL_BATCH,
    CAPABILITY_THERMAL_MAX_TEMPERATURE, CAPABILITY_TREADLE_DYNAMO_OUTPUT,
    CAPABILITY_TREADLE_POWER_OUTPUT,
};
use crate::content::crafted_parts::TIMBER_FLYWHEEL_MASS;
use crate::content::materials::{
    FORM_BOARD, FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, FORM_STONE_CROCK_BODY,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, Power, Temperature};
use crate::equipment::{EquipmentDefinition, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    power_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE, EQUIPMENT_STONE_INGOT_MOLD,
    EQUIPMENT_TIMBER_TREADLE_DRIVE, EQUIPMENT_TIMBER_TREADLE_DYNAMO,
};

const COPPER_ELECTRICAL_COMPONENT_MASS: Mass = Mass::from_milligrams(40_000);
const FIRST_FOUNDRY_BATCH: Mass = Mass::from_milligrams(20_000);

/// A timber treadle and flywheel driving a copper-wound low-voltage dynamo.
///
/// This is an explicit additive conversion of the ordinary mechanical treadle rather than a
/// second nearly identical frame. The converted machine retains its mechanical output and gains a
/// dedicated electrical provider, so earlier infrastructure remains useful while copper winding
/// still creates a real material investment before electrical thermal work becomes possible.
pub(super) fn timber_treadle_dynamo() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_TREADLE_DYNAMO,
        "copper-wound treadle dynamo",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
                TIMBER_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                COPPER_ELECTRICAL_COMPONENT_MASS,
            ),
        ]),
        profile([
            (
                CAPABILITY_TREADLE_POWER_OUTPUT,
                CapabilityValue::Power(Power::from_microwatts(100_000_000)),
            ),
            (
                CAPABILITY_TREADLE_DYNAMO_OUTPUT,
                CapabilityValue::Power(Power::from_microwatts(100_000_000)),
            ),
        ]),
        thresholds(),
        vec![
            power_condition_curve(
                CAPABILITY_TREADLE_POWER_OUTPUT,
                500_000,
                Power::from_microwatts(50_000_000),
            ),
            power_condition_curve(
                CAPABILITY_TREADLE_DYNAMO_OUTPUT,
                500_000,
                Power::from_microwatts(50_000_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                COPPER_ELECTRICAL_COMPONENT_MASS,
            ),
        ]),
    ))
}

/// Small stone crucible furnace heated by an electrical arc between replaceable copper electrodes.
///
/// The 20 g batch is intentionally tiny compared with the industrial furnace. At the first
/// dynamo's 100 W transfer rate one melt is measured in minutes, so this opens casting without
/// erasing the later industrial power and throughput problem.
pub(super) fn stone_arc_crucible_furnace() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        "stone arc crucible furnace",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
                Mass::from_milligrams(2_400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                COPPER_ELECTRICAL_COMPONENT_MASS,
            ),
        ]),
        profile([
            (
                CAPABILITY_HEATING_POWER,
                CapabilityValue::Power(Power::from_microwatts(150_000_000)),
            ),
            (
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_450_000)),
            ),
            (
                CAPABILITY_THERMAL_BATCH,
                CapabilityValue::Mass(FIRST_FOUNDRY_BATCH),
            ),
        ]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_HEATING_POWER,
            500_000,
            Power::from_microwatts(75_000_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
}

/// Carved stone mold sized to the same first-foundry batch as the arc crucible.
pub(super) fn stone_ingot_mold() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_INGOT_MOLD,
        "carved stone ingot mold",
        MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
            Mass::from_milligrams(2_400_000),
        )]),
        profile([
            (
                CAPABILITY_COOLING_POWER,
                CapabilityValue::Power(Power::from_microwatts(200_000_000)),
            ),
            (
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_450_000)),
            ),
            (
                CAPABILITY_THERMAL_BATCH,
                CapabilityValue::Mass(FIRST_FOUNDRY_BATCH),
            ),
        ]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_COOLING_POWER,
            500_000,
            Power::from_microwatts(100_000_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY))
}
