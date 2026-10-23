//! Owns built-in block and block-state content for persistent terrain.

use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::material::{CommodityKey, MaterialId, MaterialRegistry};
use crate::world::{
    BlockCollisionShape, BlockDefinition, BlockDefinitionId, BlockExcavationDefinition,
    BlockPhysicalDefinition, BlockStateDefinition, BlockStateId, WorldRegistry,
    mixed_full_cube_mass,
};

use super::materials::{
    FORM_BLOCK, FORM_CHIP, FORM_ORE, FORM_SCRAP, MATERIAL_CASSITERITE, MATERIAL_COPPER,
    MATERIAL_GALENA, MATERIAL_GRAPHITE, MATERIAL_HEMATITE, MATERIAL_IRON, MATERIAL_MAGNETITE,
    MATERIAL_PYRITE, MATERIAL_QUARTZ, MATERIAL_SPHALERITE, MATERIAL_STONE, MATERIAL_TIN,
    MATERIAL_ZINC,
};
use super::textures::{
    BLOCK_CASSITERITE_ORE, BLOCK_COPPER, BLOCK_GALENA_ORE, BLOCK_GRAPHITE_ORE, BLOCK_HEMATITE_ORE,
    BLOCK_MAGNETITE_ORE, BLOCK_PYRITE_ORE, BLOCK_QUARTZ_ORE, BLOCK_SPHALERITE_ORE, BLOCK_STONE,
    BLOCK_TEMPLE_BEARING_HOUSING, BLOCK_TEMPLE_CORROSION_PANEL, BLOCK_TEMPLE_DATA_PLATE,
    BLOCK_TEMPLE_REFRACTORY_FIXTURE,
};

pub const WORLD_BLOCK_STONE: BlockDefinitionId = BlockDefinitionId::new(2);
pub const WORLD_BLOCK_TEMPLE_ARCHIVE: BlockDefinitionId = BlockDefinitionId::new(3);
pub const WORLD_BLOCK_TEMPLE_DRIVE: BlockDefinitionId = BlockDefinitionId::new(4);
pub const WORLD_BLOCK_TEMPLE_SEAL: BlockDefinitionId = BlockDefinitionId::new(5);
pub const WORLD_BLOCK_TEMPLE_TIN_DATA_PLATE: BlockDefinitionId = BlockDefinitionId::new(6);
pub const WORLD_BLOCK_TEMPLE_IRON_BEARING_HOUSING: BlockDefinitionId = BlockDefinitionId::new(7);
pub const WORLD_BLOCK_TEMPLE_REFRACTORY_FIXTURE: BlockDefinitionId = BlockDefinitionId::new(8);
pub const WORLD_BLOCK_TEMPLE_ZINC_CORROSION_PANEL: BlockDefinitionId = BlockDefinitionId::new(9);
pub const WORLD_BLOCK_HEMATITE_ORE: BlockDefinitionId = BlockDefinitionId::new(10);
pub const WORLD_BLOCK_CASSITERITE_ORE: BlockDefinitionId = BlockDefinitionId::new(11);
pub const WORLD_BLOCK_SPHALERITE_ORE: BlockDefinitionId = BlockDefinitionId::new(12);
pub const WORLD_BLOCK_MAGNETITE_ORE: BlockDefinitionId = BlockDefinitionId::new(13);
pub const WORLD_BLOCK_GALENA_ORE: BlockDefinitionId = BlockDefinitionId::new(14);
pub const WORLD_BLOCK_PYRITE_ORE: BlockDefinitionId = BlockDefinitionId::new(15);
pub const WORLD_BLOCK_GRAPHITE_ORE: BlockDefinitionId = BlockDefinitionId::new(16);
pub const WORLD_BLOCK_QUARTZ_ORE: BlockDefinitionId = BlockDefinitionId::new(17);
pub const WORLD_BLOCK_GRADED_CASSITERITE_ORE: BlockDefinitionId = BlockDefinitionId::new(18);
pub const WORLD_BLOCK_GRADED_MAGNETITE_ORE: BlockDefinitionId = BlockDefinitionId::new(19);
pub const WORLD_BLOCK_GRADED_GALENA_ORE: BlockDefinitionId = BlockDefinitionId::new(20);

pub const BLOCK_STATE_STONE: BlockStateId = BlockStateId::new(2);
pub const BLOCK_STATE_TEMPLE_ARCHIVE: BlockStateId = BlockStateId::new(3);
pub const BLOCK_STATE_TEMPLE_DRIVE: BlockStateId = BlockStateId::new(4);
pub const BLOCK_STATE_TEMPLE_SEAL: BlockStateId = BlockStateId::new(5);
pub const BLOCK_STATE_TEMPLE_TIN_DATA_PLATE: BlockStateId = BlockStateId::new(6);
pub const BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING: BlockStateId = BlockStateId::new(7);
pub const BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE: BlockStateId = BlockStateId::new(8);
pub const BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL: BlockStateId = BlockStateId::new(9);
pub const BLOCK_STATE_HEMATITE_ORE: BlockStateId = BlockStateId::new(10);
pub const BLOCK_STATE_CASSITERITE_ORE: BlockStateId = BlockStateId::new(11);
pub const BLOCK_STATE_SPHALERITE_ORE: BlockStateId = BlockStateId::new(12);
pub const BLOCK_STATE_MAGNETITE_ORE: BlockStateId = BlockStateId::new(13);
pub const BLOCK_STATE_GALENA_ORE: BlockStateId = BlockStateId::new(14);
pub const BLOCK_STATE_PYRITE_ORE: BlockStateId = BlockStateId::new(15);
pub const BLOCK_STATE_GRAPHITE_ORE: BlockStateId = BlockStateId::new(16);
pub const BLOCK_STATE_QUARTZ_ORE: BlockStateId = BlockStateId::new(17);
pub const BLOCK_STATE_GRADED_CASSITERITE_ORE: BlockStateId = BlockStateId::new(18);
pub const BLOCK_STATE_GRADED_MAGNETITE_ORE: BlockStateId = BlockStateId::new(19);
pub const BLOCK_STATE_GRADED_GALENA_ORE: BlockStateId = BlockStateId::new(20);

pub(crate) fn build_world_registry(materials: &MaterialRegistry) -> WorldRegistry {
    WorldRegistry::new(
        [
            BlockDefinition::new(
                BlockDefinitionId::AIR,
                "air",
                BlockStateId::AIR,
                BlockPhysicalDefinition::new(None, Mass::ZERO, BlockCollisionShape::Empty, false),
                None,
            ),
            BlockDefinition::new(
                WORLD_BLOCK_STONE,
                "stone terrain",
                BLOCK_STATE_STONE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_STONE, FORM_BLOCK)),
                    full_cube_mass(materials, MATERIAL_STONE),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(350_000_000),
                    CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_STONE),
            ),
            // Buried servant-era devices are physically present but no longer powered.
            // Breaking them yields conserved copper scrap, not instant working machinery.
            BlockDefinition::new(
                WORLD_BLOCK_TEMPLE_ARCHIVE,
                "sealed temple mnemonic archive",
                BLOCK_STATE_TEMPLE_ARCHIVE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)),
                    // One thousandth of a cubic metre of metal in a full-sized housing.
                    Mass::from_milligrams(8_960_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_material_fill_fraction(1_000)
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(500_000_000),
                    CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_COPPER),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_TEMPLE_DRIVE,
                "dormant temple motive engine",
                BLOCK_STATE_TEMPLE_DRIVE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)),
                    Mass::from_milligrams(17_920_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_material_fill_fraction(2_000)
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(650_000_000),
                    CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_COPPER),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_TEMPLE_SEAL,
                "engraved servant-era temple threshold",
                BLOCK_STATE_TEMPLE_SEAL,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)),
                    Mass::from_milligrams(4_480_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_material_fill_fraction(500)
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(250_000_000),
                    CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_COPPER),
            ),
            // Finite engineering assemblies in four service bays. The metals
            // have distinct registered density, hardness and salvage custody.
            // These housings do not invent powered equipment on discovery.
            BlockDefinition::new(
                WORLD_BLOCK_TEMPLE_TIN_DATA_PLATE,
                "tin identification plate array",
                BLOCK_STATE_TEMPLE_TIN_DATA_PLATE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_TIN, FORM_BLOCK)),
                    Mass::from_milligrams(3_655_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_material_fill_fraction(500)
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(65_000_000),
                    CommodityKey::new(MATERIAL_TIN, FORM_SCRAP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_TEMPLE_DATA_PLATE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_TEMPLE_IRON_BEARING_HOUSING,
                "iron bearing housing",
                BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_IRON, FORM_BLOCK)),
                    Mass::from_milligrams(7_870_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_material_fill_fraction(1_000)
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(650_000_000),
                    CommodityKey::new(MATERIAL_IRON, FORM_SCRAP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_TEMPLE_BEARING_HOUSING),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_TEMPLE_REFRACTORY_FIXTURE,
                "stone heat-treatment fixture",
                BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_STONE, FORM_BLOCK)),
                    Mass::from_milligrams(530_000_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_material_fill_fraction(200_000)
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(600_000_000),
                    CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_TEMPLE_REFRACTORY_FIXTURE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_TEMPLE_ZINC_CORROSION_PANEL,
                "zinc sacrificial corrosion panel",
                BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_ZINC, FORM_BLOCK)),
                    Mass::from_milligrams(7_140_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_material_fill_fraction(1_000)
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(110_000_000),
                    CommodityKey::new(MATERIAL_ZINC, FORM_SCRAP),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_TEMPLE_CORROSION_PANEL),
            ),
            // Distinct compounds remain ore through mining and comminution;
            // none can be melted directly into elemental iron, tin or zinc.
            BlockDefinition::new(
                WORLD_BLOCK_HEMATITE_ORE,
                "hematite mineral deposit",
                BLOCK_STATE_HEMATITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_HEMATITE, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_HEMATITE),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(550_000_000),
                    CommodityKey::new(MATERIAL_HEMATITE, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_HEMATITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_CASSITERITE_ORE,
                "cassiterite mineral deposit",
                BLOCK_STATE_CASSITERITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_CASSITERITE, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_CASSITERITE),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(490_000_000),
                    CommodityKey::new(MATERIAL_CASSITERITE, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_CASSITERITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_SPHALERITE_ORE,
                "sphalerite mineral deposit",
                BLOCK_STATE_SPHALERITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_SPHALERITE, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_SPHALERITE),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(350_000_000),
                    CommodityKey::new(MATERIAL_SPHALERITE, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_SPHALERITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_MAGNETITE_ORE,
                "magnetite mineral deposit",
                BLOCK_STATE_MAGNETITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_MAGNETITE, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_MAGNETITE),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(515_000_000),
                    CommodityKey::new(MATERIAL_MAGNETITE, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_MAGNETITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_GALENA_ORE,
                "galena mineral deposit",
                BLOCK_STATE_GALENA_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_GALENA, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_GALENA),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(100_000_000),
                    CommodityKey::new(MATERIAL_GALENA, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_GALENA_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_PYRITE_ORE,
                "pyrite mineral deposit",
                BLOCK_STATE_PYRITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_PYRITE, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_PYRITE),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(350_000_000),
                    CommodityKey::new(MATERIAL_PYRITE, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_PYRITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_GRAPHITE_ORE,
                "graphite mineral deposit",
                BLOCK_STATE_GRAPHITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_GRAPHITE, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_GRAPHITE),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(45_000_000),
                    CommodityKey::new(MATERIAL_GRAPHITE, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_GRAPHITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_QUARTZ_ORE,
                "quartz mineral deposit",
                BLOCK_STATE_QUARTZ_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_QUARTZ, FORM_ORE)),
                    full_cube_mass(materials, MATERIAL_QUARTZ),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(BlockExcavationDefinition::new(
                    Pressure::from_pascals(700_000_000),
                    CommodityKey::new(MATERIAL_QUARTZ, FORM_ORE),
                    Temperature::from_millikelvin(293_150),
                )),
                Some(BLOCK_QUARTZ_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_GRADED_CASSITERITE_ORE,
                "cassiterite-bearing stone deposit (40% SnO2 by mass)",
                BLOCK_STATE_GRADED_CASSITERITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_CASSITERITE, FORM_ORE)),
                    mixed_full_cube_mass(materials, MATERIAL_CASSITERITE, MATERIAL_STONE, 600_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(
                    BlockExcavationDefinition::new(
                        Pressure::from_pascals(490_000_000),
                        CommodityKey::new(MATERIAL_CASSITERITE, FORM_ORE),
                        Temperature::from_millikelvin(293_150),
                    )
                    .with_gangue(MATERIAL_STONE, 600_000),
                ),
                Some(BLOCK_CASSITERITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_GRADED_MAGNETITE_ORE,
                "magnetite-bearing stone deposit (40% Fe3O4 by mass)",
                BLOCK_STATE_GRADED_MAGNETITE_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_MAGNETITE, FORM_ORE)),
                    mixed_full_cube_mass(materials, MATERIAL_MAGNETITE, MATERIAL_STONE, 600_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(
                    BlockExcavationDefinition::new(
                        Pressure::from_pascals(515_000_000),
                        CommodityKey::new(MATERIAL_MAGNETITE, FORM_ORE),
                        Temperature::from_millikelvin(293_150),
                    )
                    .with_gangue(MATERIAL_STONE, 600_000),
                ),
                Some(BLOCK_MAGNETITE_ORE),
            ),
            BlockDefinition::new(
                WORLD_BLOCK_GRADED_GALENA_ORE,
                "galena-bearing stone deposit (80% PbS by mass)",
                BLOCK_STATE_GRADED_GALENA_ORE,
                BlockPhysicalDefinition::new(
                    Some(CommodityKey::new(MATERIAL_GALENA, FORM_ORE)),
                    mixed_full_cube_mass(materials, MATERIAL_GALENA, MATERIAL_STONE, 200_000),
                    BlockCollisionShape::FullCube,
                    true,
                )
                .with_excavation(
                    BlockExcavationDefinition::new(
                        Pressure::from_pascals(100_000_000),
                        CommodityKey::new(MATERIAL_GALENA, FORM_ORE),
                        Temperature::from_millikelvin(293_150),
                    )
                    .with_gangue(MATERIAL_STONE, 200_000),
                ),
                Some(BLOCK_GALENA_ORE),
            ),
        ],
        [
            BlockStateDefinition::new(BlockStateId::AIR, BlockDefinitionId::AIR),
            BlockStateDefinition::new(BLOCK_STATE_STONE, WORLD_BLOCK_STONE),
            BlockStateDefinition::new(BLOCK_STATE_TEMPLE_ARCHIVE, WORLD_BLOCK_TEMPLE_ARCHIVE),
            BlockStateDefinition::new(BLOCK_STATE_TEMPLE_DRIVE, WORLD_BLOCK_TEMPLE_DRIVE),
            BlockStateDefinition::new(BLOCK_STATE_TEMPLE_SEAL, WORLD_BLOCK_TEMPLE_SEAL),
            BlockStateDefinition::new(
                BLOCK_STATE_TEMPLE_TIN_DATA_PLATE,
                WORLD_BLOCK_TEMPLE_TIN_DATA_PLATE,
            ),
            BlockStateDefinition::new(
                BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING,
                WORLD_BLOCK_TEMPLE_IRON_BEARING_HOUSING,
            ),
            BlockStateDefinition::new(
                BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE,
                WORLD_BLOCK_TEMPLE_REFRACTORY_FIXTURE,
            ),
            BlockStateDefinition::new(
                BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL,
                WORLD_BLOCK_TEMPLE_ZINC_CORROSION_PANEL,
            ),
            BlockStateDefinition::new(BLOCK_STATE_HEMATITE_ORE, WORLD_BLOCK_HEMATITE_ORE),
            BlockStateDefinition::new(BLOCK_STATE_CASSITERITE_ORE, WORLD_BLOCK_CASSITERITE_ORE),
            BlockStateDefinition::new(BLOCK_STATE_SPHALERITE_ORE, WORLD_BLOCK_SPHALERITE_ORE),
            BlockStateDefinition::new(BLOCK_STATE_MAGNETITE_ORE, WORLD_BLOCK_MAGNETITE_ORE),
            BlockStateDefinition::new(BLOCK_STATE_GALENA_ORE, WORLD_BLOCK_GALENA_ORE),
            BlockStateDefinition::new(BLOCK_STATE_PYRITE_ORE, WORLD_BLOCK_PYRITE_ORE),
            BlockStateDefinition::new(BLOCK_STATE_GRAPHITE_ORE, WORLD_BLOCK_GRAPHITE_ORE),
            BlockStateDefinition::new(BLOCK_STATE_QUARTZ_ORE, WORLD_BLOCK_QUARTZ_ORE),
            BlockStateDefinition::new(
                BLOCK_STATE_GRADED_CASSITERITE_ORE,
                WORLD_BLOCK_GRADED_CASSITERITE_ORE,
            ),
            BlockStateDefinition::new(
                BLOCK_STATE_GRADED_MAGNETITE_ORE,
                WORLD_BLOCK_GRADED_MAGNETITE_ORE,
            ),
            BlockStateDefinition::new(BLOCK_STATE_GRADED_GALENA_ORE, WORLD_BLOCK_GRADED_GALENA_ORE),
        ],
    )
}

fn full_cube_mass(materials: &MaterialRegistry, material: MaterialId) -> Mass {
    let density = materials
        .get_material(material)
        .unwrap_or_else(|| {
            panic!(
                "world block references missing material {}",
                material.value()
            )
        })
        .properties()
        .density_kg_per_m3();
    Mass::from_milligrams(
        u64::from(density)
            .checked_mul(1_000_000)
            .unwrap_or_else(|| panic!("full-cube material mass exceeds supported range")),
    )
}
