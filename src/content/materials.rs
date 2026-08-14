//! Built-in material and form definitions; sibling content modules assemble other registry domains.

use crate::core::quantity::Temperature;
use crate::material::{
    ElectricalProperties, FormDefinition, FormId, FusionProperties, MaterialDefinition, MaterialId,
    MaterialPhase, MaterialProperties, MaterialRegistry, MechanicalProperties, ThermalProperties,
};

pub const MATERIAL_WOOD: MaterialId = MaterialId::new(1);
pub const MATERIAL_CHARCOAL: MaterialId = MaterialId::new(2);
pub const MATERIAL_COPPER: MaterialId = MaterialId::new(3);
pub const MATERIAL_SLAG: MaterialId = MaterialId::new(4);

pub const FORM_LOG: FormId = FormId::new(1);
pub const FORM_LUMP: FormId = FormId::new(2);
pub const FORM_ORE: FormId = FormId::new(3);
pub const FORM_CONCENTRATE: FormId = FormId::new(4);
pub const FORM_INGOT: FormId = FormId::new(5);
pub const FORM_MOLTEN: FormId = FormId::new(6);
pub const FORM_CRUSHED: FormId = FormId::new(7);

pub(crate) fn build_material_registry() -> MaterialRegistry {
    let mut registry = MaterialRegistry::new();

    registry.register_form(FormDefinition::new(FORM_LOG, "log", MaterialPhase::Solid));
    registry.register_form(FormDefinition::new(FORM_LUMP, "lump", MaterialPhase::Solid));
    registry.register_form(FormDefinition::new(FORM_ORE, "ore", MaterialPhase::Solid));
    registry.register_form(FormDefinition::new(
        FORM_CONCENTRATE,
        "concentrate",
        MaterialPhase::Solid,
    ));
    registry.register_form(FormDefinition::new(
        FORM_INGOT,
        "ingot",
        MaterialPhase::Solid,
    ));
    registry.register_form(FormDefinition::new(
        FORM_MOLTEN,
        "molten",
        MaterialPhase::Liquid,
    ));
    registry.register_form(FormDefinition::new(
        FORM_CRUSHED,
        "crushed",
        MaterialPhase::Solid,
    ));

    registry.register_material(MaterialDefinition::new(
        MATERIAL_WOOD,
        "wood",
        MaterialProperties::new(
            650,
            ThermalProperties::new(1_700, None, 120),
            MechanicalProperties::new(40_000, 70_000, 30),
            ElectricalProperties::new(None),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_CHARCOAL,
        "charcoal",
        MaterialProperties::new(
            250,
            ThermalProperties::new(1_000, None, 200),
            MechanicalProperties::new(2_000, 500, 5),
            ElectricalProperties::new(None),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_COPPER,
        "copper",
        MaterialProperties::new(
            8_960,
            ThermalProperties::new(
                385,
                Some(FusionProperties::new(
                    Temperature::from_millikelvin(1_357_770),
                    205_000,
                )),
                401_000,
            ),
            MechanicalProperties::new(70_000, 210_000, 369),
            ElectricalProperties::new(Some(17)),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_SLAG,
        "slag",
        MaterialProperties::new(
            2_700,
            ThermalProperties::new(900, None, 1_000),
            MechanicalProperties::new(20_000, 2_000, 50),
            ElectricalProperties::new(None),
        ),
    ));

    registry
}
