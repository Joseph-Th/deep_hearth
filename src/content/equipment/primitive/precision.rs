//! Low-tech rotary precision tools built from already-reachable stone and timber parts.

use crate::capability::CapabilityValue;
use crate::content::capabilities::{
    CAPABILITY_COPPER_PIERCING_FLOW, CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
};
use crate::content::crafted_parts::{STONE_DRILL_BIT_MASS, STONE_FLYWHEEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_DRILL_BIT, FORM_FLYWHEEL, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::{EquipmentDefinition, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_flow_condition_curve, profile, thresholds,
};
use super::super::{EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL, EQUIPMENT_TIMBER_SPINDLE_DRILL};
use super::copper_reinforcement_input;

/// Reciprocating pump drill with a stone momentum disk and replaceable worked-stone bit.
///
/// The drill adds a precision-manufacturing role for the existing stone flywheel rather than a
/// new material tier. It remains direct player work: the flywheel smooths repeated rotary motion,
/// while the player supplies every stroke. The tool is deliberately specialized to copper-sheet
/// piercing so it does not become an ahistorical universal machining station.
pub(super) fn stone_flywheel_pump_drill() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
        "stone-flywheel pump drill",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
                STONE_DRILL_BIT_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
        ]),
        profile([(
            CAPABILITY_COPPER_PIERCING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(250)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_COPPER_PIERCING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(125),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT))
}

/// Fixed timber drill frame that reuses the pump drill's rotor, spindle stock, and stone bit.
///
/// The frame can still be worked directly by hand, but a local mechanical drive can spin the
/// spindle without reserving player attention. The upgrade therefore adds an automation choice
/// rather than replacing the learned manual transform or discarding the player's first drill.
pub(super) fn timber_spindle_drill() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        "timber spindle drill",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
                STONE_DRILL_BIT_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(800_000),
            ),
            copper_reinforcement_input(),
        ]),
        profile([
            (
                CAPABILITY_COPPER_PIERCING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(500)),
            ),
            (
                CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_500)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_COPPER_PIERCING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(250),
            ),
            mass_flow_condition_curve(
                CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(750),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            copper_reinforcement_input(),
        ]),
    ))
}
