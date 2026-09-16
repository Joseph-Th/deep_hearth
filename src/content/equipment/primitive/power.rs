//! Primitive manual-power equipment and its copper-reinforced variant.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, Power};
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{
    CAPABILITY_MANUAL_POWER_OUTPUT, CAPABILITY_TREADLE_POWER_OUTPUT,
};
use crate::content::crafted_parts::{STONE_FLYWHEEL_MASS, TIMBER_FLYWHEEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_FLYWHEEL, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD,
};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    power_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_TREADLE_DRIVE,
};
use super::{copper_reinforcement_input, copper_upgrade};

pub(super) fn stone_hand_crank() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_HAND_CRANK,
        "stone hand crank",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            CapabilityValue::Power(Power::from_microwatts(50_000_000)),
        )]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            500_000,
            Power::from_microwatts(25_000_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))
}

pub(super) fn copper_reinforced_hand_crank() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        "copper-reinforced stone hand crank",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            copper_reinforcement_input(),
        ]),
        profile([(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            CapabilityValue::Power(Power::from_microwatts(150_000_000)),
        )]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            500_000,
            Power::from_microwatts(75_000_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_HAND_CRANK))
}

/// A leg-powered alternative to the compact hand crank. It converts a much larger timber frame
/// into higher copper-free charging throughput and slightly better metabolic efficiency.
pub(super) fn timber_treadle_drive() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        "timber foot-treadle drive",
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
                Mass::from_milligrams(400_000),
            ),
        ]),
        profile([(
            CAPABILITY_TREADLE_POWER_OUTPUT,
            CapabilityValue::Power(Power::from_microwatts(100_000_000)),
        )]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_TREADLE_POWER_OUTPUT,
            500_000,
            Power::from_microwatts(50_000_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))
}
