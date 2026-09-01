//! Defines built-in materials, forms, and commodities.

use crate::core::quantity::Temperature;
use crate::material::{
    CommodityKey, FormDefinition, FormId, FusionProperties, MaterialDefinition,
    MaterialFormCohesion, MaterialId, MaterialPhase, MaterialProperties, MaterialRegistry,
    ParticleSizeStatePolicy, StructuralProperties, ThermalProperties,
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
pub const MATERIAL_LEGUMES: MaterialId = MaterialId::new(11);
pub(crate) const COPPER_MELTING_POINT: Temperature = Temperature::from_millikelvin(1_357_770);
pub(crate) const WATER_MELTING_POINT: Temperature = Temperature::from_millikelvin(273_150);
pub(crate) const WATER_LATENT_HEAT_OF_FUSION_J_PER_KG: u32 = 333_550;

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
pub const FORM_HANDLE: FormId = FormId::new(11);
pub const FORM_FLYWHEEL: FormId = FormId::new(12);
pub const FORM_REINFORCEMENT: FormId = FormId::new(13);
pub const FORM_NATIVE_METAL: FormId = FormId::new(14);
pub const FORM_SCRAP: FormId = FormId::new(15);
pub const FORM_TAILINGS: FormId = FormId::new(16);
pub const FORM_BOARD: FormId = FormId::new(17);
pub const FORM_CHEST_BODY: FormId = FormId::new(18);
pub const FORM_DOUBLE_WALL_CHEST_BODY: FormId = FormId::new(19);

fn consolidated_form(id: FormId, name: &'static str) -> FormDefinition {
    FormDefinition::new(
        id,
        name,
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
        MaterialFormCohesion::Consolidated,
    )
}

fn loose_form(
    id: FormId,
    name: &'static str,
    phase: MaterialPhase,
    particle_size_policy: ParticleSizeStatePolicy,
) -> FormDefinition {
    FormDefinition::new(
        id,
        name,
        phase,
        particle_size_policy,
        MaterialFormCohesion::Loose,
    )
}

pub(crate) fn build_material_registry() -> MaterialRegistry {
    let mut registry = MaterialRegistry::new();
    register_forms(&mut registry);
    register_materials(&mut registry);
    register_commodities(&mut registry);
    registry
}

fn register_forms(registry: &mut MaterialRegistry) {
    for definition in [
        consolidated_form(FORM_LOG, "log"),
        loose_form(
            FORM_FOOD,
            "food",
            MaterialPhase::Solid,
            ParticleSizeStatePolicy::Untracked,
        ),
        consolidated_form(FORM_TOOL, "tool"),
        loose_form(
            FORM_CHIP,
            "chip",
            MaterialPhase::Solid,
            ParticleSizeStatePolicy::Untracked,
        ),
        consolidated_form(FORM_HANDLE, "handle"),
        consolidated_form(FORM_BOARD, "board"),
        consolidated_form(FORM_CHEST_BODY, "timber chest body"),
        consolidated_form(FORM_DOUBLE_WALL_CHEST_BODY, "double-wall timber chest body"),
        consolidated_form(FORM_FLYWHEEL, "flywheel"),
        consolidated_form(FORM_REINFORCEMENT, "reinforcement"),
        loose_form(
            FORM_NATIVE_METAL,
            "native metal",
            MaterialPhase::Solid,
            ParticleSizeStatePolicy::Untracked,
        ),
        loose_form(
            FORM_SCRAP,
            "scrap",
            MaterialPhase::Solid,
            ParticleSizeStatePolicy::Untracked,
        ),
        consolidated_form(FORM_LUMP, "lump"),
        consolidated_form(FORM_ORE, "ore"),
        loose_form(
            FORM_CONCENTRATE,
            "concentrate",
            MaterialPhase::Solid,
            ParticleSizeStatePolicy::Required,
        ),
        consolidated_form(FORM_INGOT, "ingot"),
        loose_form(
            FORM_MOLTEN,
            "molten",
            MaterialPhase::Liquid,
            ParticleSizeStatePolicy::Untracked,
        ),
        loose_form(
            FORM_CRUSHED,
            "crushed",
            MaterialPhase::Solid,
            ParticleSizeStatePolicy::Required,
        ),
        loose_form(
            FORM_TAILINGS,
            "tailings",
            MaterialPhase::Solid,
            ParticleSizeStatePolicy::Required,
        ),
    ] {
        registry.register_form(definition);
    }
}

fn register_materials(registry: &mut MaterialRegistry) {
    for definition in [
        MaterialDefinition::new(
            MATERIAL_WOOD,
            "wood",
            MaterialProperties::new(
                650,
                ThermalProperties::new(1_700, None),
                Some(StructuralProperties::new(40_000, 70_000)),
            ),
        ),
        MaterialDefinition::new(
            MATERIAL_CHARCOAL,
            "charcoal",
            MaterialProperties::new(250, ThermalProperties::new(1_000, None), None),
        ),
        MaterialDefinition::new(
            MATERIAL_COPPER,
            "copper",
            MaterialProperties::new(
                8_960,
                ThermalProperties::new(
                    385,
                    Some(FusionProperties::new(COPPER_MELTING_POINT, 205_000)),
                ),
                Some(StructuralProperties::new(70_000, 210_000)),
            ),
        ),
        MaterialDefinition::new(
            MATERIAL_SLAG,
            "slag",
            MaterialProperties::new(2_700, ThermalProperties::new(900, None), None),
        ),
        MaterialDefinition::new(
            MATERIAL_WATER,
            "water",
            MaterialProperties::new(
                1_000,
                ThermalProperties::new(
                    4_184,
                    Some(FusionProperties::new(
                        WATER_MELTING_POINT,
                        WATER_LATENT_HEAT_OF_FUSION_J_PER_KG,
                    )),
                ),
                None,
            ),
        ),
        MaterialDefinition::new(
            MATERIAL_GRAIN,
            "grain",
            MaterialProperties::new(750, ThermalProperties::new(1_500, None), None),
        ),
        MaterialDefinition::new(
            MATERIAL_BERRIES,
            "berries",
            MaterialProperties::new(1_000, ThermalProperties::new(3_800, None), None),
        ),
        MaterialDefinition::new(
            MATERIAL_MEAT,
            "meat",
            MaterialProperties::new(1_050, ThermalProperties::new(3_300, None), None),
        ),
        MaterialDefinition::new(
            MATERIAL_STONE,
            "stone",
            MaterialProperties::new(
                2_650,
                ThermalProperties::new(800, None),
                Some(StructuralProperties::new(100_000, 10_000)),
            ),
        ),
        MaterialDefinition::new(
            MATERIAL_CLAY,
            "clay",
            MaterialProperties::new(1_900, ThermalProperties::new(900, None), None),
        ),
        MaterialDefinition::new(
            MATERIAL_LEGUMES,
            "roasted legumes",
            MaterialProperties::new(800, ThermalProperties::new(1_600, None), None),
        ),
    ] {
        registry.register_material(definition);
    }
}

fn register_commodities(registry: &mut MaterialRegistry) {
    for commodity in [
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE),
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        CommodityKey::new(MATERIAL_SLAG, FORM_LUMP),
        CommodityKey::new(MATERIAL_SLAG, FORM_CRUSHED),
        CommodityKey::new(MATERIAL_SLAG, FORM_TAILINGS),
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
        CommodityKey::new(MATERIAL_LEGUMES, FORM_FOOD),
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED),
        CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS),
        CommodityKey::new(MATERIAL_CLAY, FORM_CRUSHED),
        CommodityKey::new(MATERIAL_CLAY, FORM_TAILINGS),
    ] {
        registry.register_commodity(commodity);
    }
}
