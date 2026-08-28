//! Built-in immutable game definitions; sibling files own authored definitions for each registry domain.

mod capabilities;
mod crafting;
mod energy;
mod equipment;
mod fluid;
#[cfg(feature = "test-gameplay")]
#[doc(hidden)]
pub mod gameplay_fixture;
mod labor;
mod materials;
mod mining;
mod ore_processing;
mod processes;
mod shaders;
mod structural;
mod survival;
#[cfg(test)]
mod test_support;
mod textures;
mod thermal;

use crate::core::quantity::Acceleration;
use crate::core::time::CalendarDefinition;
use crate::registry::{
    CoreDefinitions, Registries, RegistryDomains, RegistryPresentation, RegistrySchemaVersion,
};
pub use fluid::FLUID_WATER;
pub use labor::{
    MANUAL_POWER_HAND_CRANK, PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    PROSPECTING_REGIONAL_RECONNAISSANCE,
};
pub use mining::MINING_METHOD_HAND_PICK;
#[cfg(test)]
use test_support::{
    empty_energy_registry, empty_equipment_registry, empty_shader_registry, empty_texture_registry,
    empty_thermal_registry,
};
#[cfg(test)]
pub(crate) use test_support::{
    make_test_registries_with_casting, make_test_registries_with_comminution,
    make_test_registries_with_energy_store, make_test_registries_with_equipment,
    make_test_registries_with_fluids, make_test_registries_with_melting,
    make_test_registries_with_process, make_test_registries_with_screening,
    make_test_registries_with_sensible_heating,
};

pub use energy::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_THERMAL_SINK,
};
pub use equipment::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE,
    EQUIPMENT_GRAVITY_SEPARATOR, EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_SEPARATOR,
};
pub use materials::{
    FORM_CHIP, FORM_CONCENTRATE, FORM_CRUSHED, FORM_FLYWHEEL, FORM_FOOD, FORM_HANDLE, FORM_INGOT,
    FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_SCRAP,
    FORM_TOOL, MATERIAL_BERRIES, MATERIAL_CHARCOAL, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_GRAIN,
    MATERIAL_LEGUMES, MATERIAL_MEAT, MATERIAL_SLAG, MATERIAL_STONE, MATERIAL_WATER, MATERIAL_WOOD,
};
pub use processes::{
    PROCESS_CAST_PURE_COPPER, PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CONCENTRATE_COPPER,
    PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_KNAP_STONE_TOOL, PROCESS_MELT_PURE_COPPER, PROCESS_SCREEN_CRUSHED_ORE,
    PROCESS_SEPARATE_NATIVE_COPPER, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE,
};
#[cfg(feature = "test-shader-validation")]
pub use shaders::{BuiltInShaderValidationError, validate_builtin_shader_programs};
pub use shaders::{
    SHADER_BLOOM, SHADER_LIGHT_CULL, SHADER_POST_PROCESS, SHADER_SHADOW, SHADER_SHADOW_CUTOUT,
    SHADER_SKY, SHADER_SMOKE, SHADER_SURFACE, SHADER_WATER,
};
pub use structural::{STRUCTURAL_PROFILE_AXIAL_COMPRESSION, STRUCTURAL_PROFILE_AXIAL_TENSION};
pub use textures::{
    BLOCK_CHARCOAL, BLOCK_COPPER, BLOCK_COPPER_ORE, BLOCK_SLAG, BLOCK_TIMBER, OBJECT_CASTING_MOLD,
    OBJECT_CHARCOAL, OBJECT_COPPER_INGOT, OBJECT_COPPER_ORE, OBJECT_COPPER_REINFORCED_HAND_CRANK,
    OBJECT_COPPER_REINFORCED_PICK, OBJECT_COPPER_REINFORCEMENT, OBJECT_COPPER_SCRAP,
    OBJECT_CRUSHED_ORE, OBJECT_DRY_SCREEN, OBJECT_ELECTRIC_FURNACE, OBJECT_GRAVITY_SEPARATOR,
    OBJECT_GRINDING_MILL, OBJECT_JAW_CRUSHER, OBJECT_LOG, OBJECT_MOLTEN_COPPER,
    OBJECT_NATIVE_COPPER, OBJECT_SLAG, OBJECT_STONE_CHIP, OBJECT_STONE_CRUSHER,
    OBJECT_STONE_FLYWHEEL, OBJECT_STONE_HAND_CRANK, OBJECT_STONE_LUMP, OBJECT_STONE_PICK,
    OBJECT_STONE_SEPARATOR, OBJECT_STONE_TOOL, OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE,
    TEXTURE_CHARCOAL, TEXTURE_COPPER_HAMMERED, TEXTURE_COPPER_ORE, TEXTURE_CRUSHED_ORE,
    TEXTURE_MACHINE_PANEL, TEXTURE_MOLTEN_COPPER, TEXTURE_REFRACTORY, TEXTURE_SCREEN_MESH,
    TEXTURE_SLAG, TEXTURE_STONE, TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE, TEXTURE_WORKING_METAL,
};

const DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED: u64 = 9_806_650;
const DEFAULT_TICKS_PER_DAY: u64 = 24_000;
const DEFAULT_PHYSICAL_SECONDS_PER_DAY: u32 = 86_400;
const DEFAULT_DAYS_PER_MONTH: u16 = 8;
const DEFAULT_MONTHS_PER_YEAR: u16 = 12;
const REGISTRY_SCHEMA_VERSION: RegistrySchemaVersion = RegistrySchemaVersion::new(55);

fn build_core_definitions() -> CoreDefinitions {
    CoreDefinitions::new(
        Acceleration::from_micrometers_per_second_squared(
            DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED,
        ),
        CalendarDefinition::new(
            DEFAULT_TICKS_PER_DAY,
            DEFAULT_PHYSICAL_SECONDS_PER_DAY,
            DEFAULT_DAYS_PER_MONTH,
            DEFAULT_MONTHS_PER_YEAR,
        ),
    )
}

/// Builds the immutable built-in registry set used by a new application instance.
#[must_use]
pub fn build_registries() -> Registries {
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: energy::build_energy_registry(),
            fluid: fluid::build_fluid_registry(),
            capabilities: capabilities::build_capability_registry(),
            crafting: crafting::build_crafting_registry(),
            labor: labor::build_labor_registry(),
            equipment: equipment::build_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            mining: mining::build_mining_registry(),
            ore_processing: ore_processing::build_ore_processing_registry(),
            thermal: thermal::build_thermal_registry(),
            production: processes::build_production_registry(),
            survival: survival::build_survival_registry(),
            presentation: RegistryPresentation {
                textures: textures::build_texture_registry(),
                shaders: shaders::build_shader_registry(),
            },
        },
    )
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
