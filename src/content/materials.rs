//! Built-in material and form definitions; sibling content modules assemble other registry domains.

use crate::core::quantity::Temperature;
use crate::material::{
    ElectricalProperties, FormDefinition, FormId, FusionProperties, MaterialDefinition, MaterialId,
    MaterialPhase, MaterialProperties, MaterialRegistry, MechanicalProperties,
    ParticleSizeStatePolicy, ThermalProperties,
};

pub const MATERIAL_WOOD: MaterialId = MaterialId::new(1);
pub const MATERIAL_CHARCOAL: MaterialId = MaterialId::new(2);
pub const MATERIAL_COPPER: MaterialId = MaterialId::new(3);
pub const MATERIAL_SLAG: MaterialId = MaterialId::new(4);
pub const MATERIAL_WATER: MaterialId = MaterialId::new(5);
pub const MATERIAL_GRAIN: MaterialId = MaterialId::new(6);
pub const MATERIAL_BERRIES: MaterialId = MaterialId::new(7);
pub const MATERIAL_MEAT: MaterialId = MaterialId::new(8);
pub const MATERIAL_STONE: MaterialId = MaterialId::new(9);
pub const MATERIAL_CLAY: MaterialId = MaterialId::new(10);

pub const FORM_LOG: FormId = FormId::new(1);
pub const FORM_LUMP: FormId = FormId::new(2);
pub const FORM_ORE: FormId = FormId::new(3);
pub const FORM_CONCENTRATE: FormId = FormId::new(4);
pub const FORM_INGOT: FormId = FormId::new(5);
pub const FORM_MOLTEN: FormId = FormId::new(6);
pub const FORM_CRUSHED: FormId = FormId::new(7);
pub const FORM_FOOD: FormId = FormId::new(8);
pub const FORM_TOOL: FormId = FormId::new(9);
pub const FORM_CHIP: FormId = FormId::new(10);
pub const FORM_HANDLE: FormId = FormId::new(12);
pub const FORM_FLYWHEEL: FormId = FormId::new(13);
pub const FORM_REINFORCEMENT: FormId = FormId::new(14);
pub const FORM_NATIVE_METAL: FormId = FormId::new(15);
pub const FORM_SCRAP: FormId = FormId::new(16);

pub(crate) fn build_material_registry() -> MaterialRegistry {
    let mut registry = MaterialRegistry::new();

    registry.register_form(FormDefinition::new(
        FORM_LOG,
        "log",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_FOOD,
        "food",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_TOOL,
        "tool",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_CHIP,
        "chip",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_HANDLE,
        "handle",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_FLYWHEEL,
        "flywheel",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_REINFORCEMENT,
        "reinforcement",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_NATIVE_METAL,
        "native metal",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_SCRAP,
        "scrap",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_LUMP,
        "lump",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_ORE,
        "ore",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_CONCENTRATE,
        "concentrate",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_INGOT,
        "ingot",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_MOLTEN,
        "molten",
        MaterialPhase::Liquid,
        ParticleSizeStatePolicy::Untracked,
    ));
    registry.register_form(FormDefinition::new(
        FORM_CRUSHED,
        "crushed",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Required,
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
    registry.register_material(MaterialDefinition::new(
        MATERIAL_WATER,
        "water",
        MaterialProperties::new(
            1_000,
            ThermalProperties::new(4_184, None, 600),
            MechanicalProperties::new(1, 1, 1),
            ElectricalProperties::new(None),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_GRAIN,
        "grain",
        MaterialProperties::new(
            750,
            ThermalProperties::new(1_500, None, 150),
            MechanicalProperties::new(2_000, 1_000, 10),
            ElectricalProperties::new(None),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_BERRIES,
        "berries",
        MaterialProperties::new(
            1_000,
            ThermalProperties::new(3_800, None, 500),
            MechanicalProperties::new(500, 100, 2),
            ElectricalProperties::new(None),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_MEAT,
        "meat",
        MaterialProperties::new(
            1_050,
            ThermalProperties::new(3_300, None, 450),
            MechanicalProperties::new(1_000, 500, 5),
            ElectricalProperties::new(None),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_STONE,
        "stone",
        MaterialProperties::new(
            2_650,
            ThermalProperties::new(800, None, 700),
            MechanicalProperties::new(100_000, 10_000, 50_000),
            ElectricalProperties::new(None),
        ),
    ));
    registry.register_material(MaterialDefinition::new(
        MATERIAL_CLAY,
        "clay",
        MaterialProperties::new(
            1_900,
            ThermalProperties::new(900, None, 500),
            MechanicalProperties::new(5_000, 1_000, 50),
            ElectricalProperties::new(None),
        ),
    ));

    registry
}
