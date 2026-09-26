//! Defines built-in materials, forms, and commodities.

use crate::core::quantity::Temperature;
use crate::material::{
    CommodityKey, FormDefinition, FormId, FusionProperties, MaterialDefinition,
    MaterialFormCohesion, MaterialId, MaterialPhase, MaterialProperties, MaterialRegistry,
    ParticleSizeStatePolicy, StructuralProperties, ThermalProperties,
};

pub const MATERIAL_WOOD: MaterialId = MaterialId::new(1);
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
pub const FORM_BULK_CRATE_BODY: FormId = FormId::new(20);
pub const FORM_INSULATED_PANTRY_BODY: FormId = FormId::new(21);
pub const FORM_ROUGH_BOX_BODY: FormId = FormId::new(22);
pub const FORM_STONE_CROCK_BODY: FormId = FormId::new(23);
pub const FORM_SCREEN_PLATE: FormId = FormId::new(24);
pub const FORM_SAW_BLADE: FormId = FormId::new(25);
pub const FORM_TIMBER_RIDDLE_PANEL: FormId = FormId::new(26);
pub const FORM_EXHAUSTED_TAILINGS: FormId = FormId::new(27);
pub const FORM_DRILL_BIT: FormId = FormId::new(28);
pub const FORM_GRINDSTONE_WHEEL: FormId = FormId::new(29);

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
        consolidated_form(FORM_BULK_CRATE_BODY, "bulk timber crate body"),
        consolidated_form(FORM_INSULATED_PANTRY_BODY, "insulated timber pantry body"),
        consolidated_form(FORM_ROUGH_BOX_BODY, "rough timber field box body"),
        consolidated_form(FORM_STONE_CROCK_BODY, "carved stone provisions crock body"),
        consolidated_form(FORM_SCREEN_PLATE, "perforated sizing screen plate"),
        consolidated_form(FORM_SAW_BLADE, "toothed frame-saw blade"),
        consolidated_form(FORM_TIMBER_RIDDLE_PANEL, "timber riddle panel"),
        consolidated_form(FORM_DRILL_BIT, "knapped rotary drill bit"),
        consolidated_form(FORM_GRINDSTONE_WHEEL, "abrasive grindstone wheel"),
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
        loose_form(
            FORM_EXHAUSTED_TAILINGS,
            "exhausted tailings",
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
    for (commodity, name) in [
        (CommodityKey::new(MATERIAL_WOOD, FORM_LOG), "timber log"),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), "wood handle"),
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), "timber board"),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            "timber chest body",
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            "double-wall timber chest body",
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
            "bulk timber crate body",
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
            "insulated timber pantry body",
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
            "rough timber field box body",
        ),
        (CommodityKey::new(MATERIAL_WOOD, FORM_CHIP), "wood chips"),
        (CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP), "wood scrap"),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
            "timber riddle panel",
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
            "timber flywheel",
        ),
        (CommodityKey::new(MATERIAL_COPPER, FORM_ORE), "copper ore"),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            "crushed copper ore",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE),
            "copper concentrate",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
            "copper ingot",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            "molten copper",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            "copper reinforcement",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
            "copper sizing screen plate",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
            "copper frame-saw blade",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            "native copper",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
            "copper scrap",
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_CHIP),
            "copper chips",
        ),
        (
            CommodityKey::new(MATERIAL_SLAG, FORM_CRUSHED),
            "crushed slag",
        ),
        (
            CommodityKey::new(MATERIAL_SLAG, FORM_TAILINGS),
            "slag tailings",
        ),
        (
            CommodityKey::new(MATERIAL_SLAG, FORM_EXHAUSTED_TAILINGS),
            "exhausted slag tailings",
        ),
        (CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD), "grain"),
        (CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD), "berries"),
        (CommodityKey::new(MATERIAL_MEAT, FORM_FOOD), "meat"),
        (
            CommodityKey::new(MATERIAL_LEGUMES, FORM_FOOD),
            "roasted legumes",
        ),
        (CommodityKey::new(MATERIAL_STONE, FORM_LUMP), "stone"),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
            "carved stone provisions crock body",
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            "knapped stone tool",
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
            "knapped stone drill bit",
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL),
            "stone grindstone wheel",
        ),
        (CommodityKey::new(MATERIAL_STONE, FORM_CHIP), "stone chips"),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            "stone flywheel",
        ),
        (CommodityKey::new(MATERIAL_STONE, FORM_SCRAP), "stone scrap"),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED),
            "crushed stone",
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS),
            "stone tailings",
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_EXHAUSTED_TAILINGS),
            "exhausted stone tailings",
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_CRUSHED),
            "crushed clay",
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_TAILINGS),
            "clay tailings",
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_EXHAUSTED_TAILINGS),
            "exhausted clay tailings",
        ),
    ] {
        registry.register_commodity(commodity, name);
    }
}
