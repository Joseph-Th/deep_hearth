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
pub use labor::MANUAL_POWER_HAND_CRANK;
pub use mining::MINING_METHOD_HAND_PICK;
#[cfg(test)]
use test_support::{
    empty_energy_registry, empty_equipment_registry, empty_shader_registry, empty_texture_registry,
    empty_thermal_registry,
};
#[cfg(test)]
pub(crate) use test_support::{
    make_test_registries_with_casting, make_test_registries_with_comminution,
    make_test_registries_with_energy_store, make_test_registries_with_energy_stores,
    make_test_registries_with_energy_stores_and_process, make_test_registries_with_equipment,
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
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, EQUIPMENT_STONE_CRUSHER,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
};
pub use materials::{
    FORM_CHIP, FORM_CONCENTRATE, FORM_CRUSHED, FORM_FLYWHEEL, FORM_FOOD, FORM_HANDLE, FORM_INGOT,
    FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_SCRAP,
    FORM_TOOL, FORM_UNFIRED_POTTERY, MATERIAL_BERRIES, MATERIAL_CHARCOAL, MATERIAL_CLAY,
    MATERIAL_COPPER, MATERIAL_GRAIN, MATERIAL_MEAT, MATERIAL_SLAG, MATERIAL_STONE, MATERIAL_WATER,
    MATERIAL_WOOD,
};
pub use processes::{
    PROCESS_CAST_PURE_COPPER, PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_FORM_CLAY_VESSEL, PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_KNAP_STONE_TOOL, PROCESS_MELT_PURE_COPPER, PROCESS_SCREEN_CRUSHED_ORE,
    PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE,
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
    OBJECT_CRUSHED_ORE, OBJECT_DRY_SCREEN, OBJECT_ELECTRIC_FURNACE, OBJECT_GRINDING_MILL,
    OBJECT_JAW_CRUSHER, OBJECT_LOG, OBJECT_MOLTEN_COPPER, OBJECT_NATIVE_COPPER, OBJECT_SLAG,
    OBJECT_STONE_CHIP, OBJECT_STONE_CRUSHER, OBJECT_STONE_FLYWHEEL, OBJECT_STONE_HAND_CRANK,
    OBJECT_STONE_LUMP, OBJECT_STONE_PICK, OBJECT_STONE_TOOL, OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE,
    TEXTURE_CHARCOAL, TEXTURE_COPPER_HAMMERED, TEXTURE_COPPER_ORE, TEXTURE_CRUSHED_ORE,
    TEXTURE_MACHINE_PANEL, TEXTURE_MOLTEN_COPPER, TEXTURE_REFRACTORY, TEXTURE_SCREEN_MESH,
    TEXTURE_SLAG, TEXTURE_STONE, TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE, TEXTURE_WORKING_METAL,
};

const DEFAULT_TICKS_PER_SECOND: u16 = 20;
const DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED: u64 = 9_806_650;
const DEFAULT_TICKS_PER_DAY: u64 = 24_000;
const DEFAULT_DAYS_PER_MONTH: u16 = 8;
const DEFAULT_MONTHS_PER_YEAR: u16 = 12;
const REGISTRY_SCHEMA_VERSION: RegistrySchemaVersion = RegistrySchemaVersion::new(30);

fn build_core_definitions() -> CoreDefinitions {
    CoreDefinitions::new(
        DEFAULT_TICKS_PER_SECOND,
        Acceleration::from_micrometers_per_second_squared(
            DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED,
        ),
        CalendarDefinition::new(
            DEFAULT_TICKS_PER_DAY,
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
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityRegistry,
        CapabilityRequirement, CapabilityValue, CapabilityValueKind,
    };
    use crate::core::quantity::{Length, Mass, MassSpecificEnergy, Temperature};
    use crate::energy::EnergyCarrier;
    use crate::material::{CommodityKey, MaterialInputSpec, ParticleSizeRange};
    use crate::ore_processing::{
        ComminutionOperatingProfile, ComminutionProcessDefinition, OreProcessingRegistry,
    };
    use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};
    use crate::thermal::{SensibleHeatingProcessDefinition, ThermalRegistry};

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(700_001);
    const TEST_PROCESS: ProcessId = ProcessId::new(700_001);
    const TEST_MASS_FLOW: CapabilityId = CapabilityId::new(700_002);
    const TEST_MAX_BATCH_MASS: CapabilityId = CapabilityId::new(700_003);
    const TEST_HEATING_POWER: CapabilityId = CapabilityId::new(700_004);
    const TEST_MAX_TEMPERATURE: CapabilityId = CapabilityId::new(700_005);

    #[test]
    fn built_in_tick_rate_is_nonzero_and_stable() {
        let registries = build_registries();

        assert_eq!(registries.core().ticks_per_second().get(), 20);
        assert_eq!(
            registries.core().gravity().micrometers_per_second_squared(),
            DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED
        );
        assert_eq!(
            registries.core().calendar().ticks_per_day(),
            DEFAULT_TICKS_PER_DAY
        );
    }

    #[test]
    fn built_in_workshop_ids_resolve_canonical_gameplay_content() {
        let registries = build_registries();

        for equipment in [
            EQUIPMENT_JAW_CRUSHER,
            EQUIPMENT_ELECTRIC_FURNACE,
            EQUIPMENT_CASTING_MOLD,
            EQUIPMENT_DRY_SCREEN,
            EQUIPMENT_GRINDING_MILL,
            EQUIPMENT_STONE_CRUSHER,
        ] {
            assert!(registries.equipment().get_equipment(equipment).is_some());
        }
        for energy in [
            ENERGY_MECHANICAL_SMALL_DRIVE,
            ENERGY_MECHANICAL_LARGE_DRIVE,
            ENERGY_ELECTRICAL_BUFFER,
            ENERGY_THERMAL_SINK,
            ENERGY_STONE_FLYWHEEL_DRIVE,
        ] {
            assert!(registries.energy().get_store(energy).is_some());
        }
        for process in [
            PROCESS_CRUSH_ORE,
            PROCESS_MELT_PURE_COPPER,
            PROCESS_CAST_PURE_COPPER,
            PROCESS_SCREEN_CRUSHED_ORE,
            PROCESS_GRIND_CRUSHED_ORE,
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        ] {
            assert!(registries.production().get_process(process).is_some());
        }
        assert!(
            registries
                .ore_processing()
                .get_comminution(PROCESS_CRUSH_ORE)
                .is_some()
        );
        assert!(
            registries
                .ore_processing()
                .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
                .is_some()
        );
        assert!(
            registries
                .ore_processing()
                .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
                .is_some()
        );
        assert!(
            registries
                .ore_processing()
                .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
                .is_some()
        );
        assert!(
            registries
                .thermal()
                .get_melting(PROCESS_MELT_PURE_COPPER)
                .is_some()
        );
        assert!(
            registries
                .thermal()
                .get_casting(PROCESS_CAST_PURE_COPPER)
                .is_some()
        );
    }

    #[test]
    fn built_in_texture_bindings_resolve_for_material_forms_and_equipment() {
        let registries = build_registries();
        let textures = registries.textures();
        let baked = textures.bake_texture_array();

        for (commodity, block, object) in [
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Some(BLOCK_TIMBER),
                OBJECT_LOG,
            ),
            (
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Some(BLOCK_CHARCOAL),
                OBJECT_CHARCOAL,
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
                Some(BLOCK_COPPER_ORE),
                OBJECT_COPPER_ORE,
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
                None,
                OBJECT_CRUSHED_ORE,
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
                Some(BLOCK_COPPER),
                OBJECT_COPPER_INGOT,
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                None,
                OBJECT_COPPER_REINFORCEMENT,
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
                None,
                OBJECT_NATIVE_COPPER,
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                None,
                OBJECT_COPPER_SCRAP,
            ),
            (
                CommodityKey::new(MATERIAL_SLAG, FORM_LUMP),
                Some(BLOCK_SLAG),
                OBJECT_SLAG,
            ),
            (
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                None,
                OBJECT_STONE_LUMP,
            ),
            (
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                None,
                OBJECT_STONE_TOOL,
            ),
            (
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                None,
                OBJECT_STONE_FLYWHEEL,
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                None,
                OBJECT_WOOD_HANDLE,
            ),
        ] {
            let binding = match textures.get_commodity_appearance(commodity) {
                Some(binding) => binding,
                None => panic!("missing commodity appearance {}", commodity.value()),
            };
            assert_eq!(binding.block(), block);
            assert_eq!(binding.object(), Some(object));
            if let Some(block) = block {
                let baked_block = match baked.get_block(block) {
                    Some(block) => block,
                    None => panic!("missing baked block appearance {}", block.value()),
                };
                let authored_block = match textures.get_block(block) {
                    Some(block) => block,
                    None => panic!("missing authored block appearance {}", block.value()),
                };
                let top_texture = authored_block.texture(crate::texture::CubeFace::Top);
                assert_eq!(
                    baked_block.texture(crate::texture::CubeFace::Top),
                    match baked.get_descriptor(top_texture) {
                        Some(descriptor) => descriptor,
                        None => panic!("missing baked texture {}", top_texture.value()),
                    }
                );
            }
        }

        for (equipment, object) in [
            (EQUIPMENT_JAW_CRUSHER, OBJECT_JAW_CRUSHER),
            (EQUIPMENT_ELECTRIC_FURNACE, OBJECT_ELECTRIC_FURNACE),
            (EQUIPMENT_CASTING_MOLD, OBJECT_CASTING_MOLD),
            (EQUIPMENT_DRY_SCREEN, OBJECT_DRY_SCREEN),
            (EQUIPMENT_GRINDING_MILL, OBJECT_GRINDING_MILL),
            (EQUIPMENT_STONE_PICK, OBJECT_STONE_PICK),
            (EQUIPMENT_STONE_HAND_CRANK, OBJECT_STONE_HAND_CRANK),
            (EQUIPMENT_STONE_CRUSHER, OBJECT_STONE_CRUSHER),
        ] {
            let binding = match textures.get_equipment_appearance(equipment) {
                Some(binding) => binding,
                None => panic!("missing equipment appearance {}", equipment.value()),
            };
            assert_eq!(binding.object(), object);
            let appearance = match textures.get_object(object) {
                Some(appearance) => appearance,
                None => panic!("missing object appearance {}", object.value()),
            };
            for texture in appearance.textures() {
                assert!(baked.get_descriptor(*texture).is_some());
            }
            let baked_object = match baked.get_object(object) {
                Some(appearance) => appearance,
                None => panic!("missing baked object appearance {}", object.value()),
            };
            assert_eq!(baked_object.textures().len(), appearance.textures().len());
        }
    }

    #[test]
    fn process_capability_references_are_validated_during_registry_assembly() {
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register_capability(CapabilityDefinition::new(
            TEST_CAPABILITY,
            "test chamber temperature",
            CapabilityValueKind::Temperature,
        ));
        let process = ProcessDefinition::new(
            TEST_PROCESS,
            "test capability process",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1),
            )],
            vec![CapabilityRequirement::new(
                TEST_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(500_000)),
            )],
        );
        let mut production = ProductionRegistry::new();
        production.register_process_for_test(process);

        let registries = Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            RegistryDomains {
                energy: empty_energy_registry(),
                fluid: fluid::build_fluid_registry(),
                capabilities,
                crafting: crate::crafting::CraftingRegistry::new(std::iter::empty()),
                labor: labor::empty_labor_registry(),
                equipment: empty_equipment_registry(),
                structural: structural::build_structural_registry(),
                materials: materials::build_material_registry(),
                mining: crate::mining::MiningRegistry::new(std::iter::empty()),
                ore_processing: OreProcessingRegistry::new(std::iter::empty()),
                thermal: empty_thermal_registry(),
                production,
                survival: survival::build_survival_registry(),
                presentation: RegistryPresentation {
                    textures: empty_texture_registry(),
                    shaders: empty_shader_registry(),
                },
            },
        );

        assert!(
            registries
                .capabilities()
                .get_capability(TEST_CAPABILITY)
                .is_some()
        );
        assert!(registries.production().get_process(TEST_PROCESS).is_some());
    }

    #[test]
    fn process_cannot_own_multiple_physical_resolver_semantics() {
        let mut capabilities = CapabilityRegistry::new();
        for (id, name, kind) in [
            (
                TEST_MASS_FLOW,
                "test mass flow",
                CapabilityValueKind::MassFlow,
            ),
            (
                TEST_MAX_BATCH_MASS,
                "test maximum batch mass",
                CapabilityValueKind::Mass,
            ),
            (
                TEST_HEATING_POWER,
                "test heating power",
                CapabilityValueKind::Power,
            ),
            (
                TEST_MAX_TEMPERATURE,
                "test maximum temperature",
                CapabilityValueKind::Temperature,
            ),
        ] {
            capabilities.register_capability(CapabilityDefinition::new(id, name, kind));
        }
        let process = ProcessDefinition::new_selected_batch(
            TEST_PROCESS,
            "ambiguous physical resolver fixture",
            Vec::new(),
        );
        let mut production = ProductionRegistry::new();
        production.register_process_for_test(process);
        let ore_processing = OreProcessingRegistry::new([ComminutionProcessDefinition::new(
            TEST_PROCESS,
            FORM_ORE,
            FORM_CRUSHED,
            match ParticleSizeRange::new(
                Length::from_micrometers(1),
                Length::from_micrometers(20_000),
            ) {
                Ok(range) => range,
                Err(error) => panic!("comminution particle-size fixture failed: {error}"),
            },
            ComminutionOperatingProfile::new(
                TEST_MASS_FLOW,
                TEST_MAX_BATCH_MASS,
                EnergyCarrier::Mechanical,
                MassSpecificEnergy::from_nanojoules_per_milligram(1),
                1,
            ),
        )]);
        let thermal = ThermalRegistry::new(
            [SensibleHeatingProcessDefinition::new(
                TEST_PROCESS,
                TEST_HEATING_POWER,
                TEST_MAX_TEMPERATURE,
                TEST_MAX_BATCH_MASS,
                EnergyCarrier::Electrical,
                1,
            )],
            std::iter::empty(),
            std::iter::empty(),
        );

        let result = std::panic::catch_unwind(|| {
            Registries::new(
                REGISTRY_SCHEMA_VERSION,
                build_core_definitions(),
                RegistryDomains {
                    energy: empty_energy_registry(),
                    fluid: fluid::build_fluid_registry(),
                    capabilities,
                    crafting: crate::crafting::CraftingRegistry::new(std::iter::empty()),
                    labor: labor::empty_labor_registry(),
                    equipment: empty_equipment_registry(),
                    structural: structural::build_structural_registry(),
                    materials: materials::build_material_registry(),
                    mining: crate::mining::MiningRegistry::new(std::iter::empty()),
                    ore_processing,
                    thermal,
                    production,
                    survival: survival::build_survival_registry(),
                    presentation: RegistryPresentation {
                        textures: empty_texture_registry(),
                        shaders: empty_shader_registry(),
                    },
                },
            )
        });

        assert!(result.is_err());
    }
}
