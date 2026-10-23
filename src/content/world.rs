//! Owns built-in block and block-state content for persistent terrain.

use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::material::{CommodityKey, MaterialId, MaterialRegistry};
use crate::world::{
    BlockCollisionShape, BlockDefinition, BlockDefinitionId, BlockExcavationDefinition,
    BlockPhysicalDefinition, BlockStateDefinition, BlockStateId, WorldRegistry,
};

use super::materials::{
    FORM_BLOCK, FORM_CHIP, FORM_SCRAP, MATERIAL_COPPER, MATERIAL_IRON, MATERIAL_STONE,
    MATERIAL_TIN, MATERIAL_ZINC,
};
use super::textures::{
    BLOCK_COPPER, BLOCK_STONE, BLOCK_TEMPLE_BEARING_HOUSING, BLOCK_TEMPLE_CORROSION_PANEL,
    BLOCK_TEMPLE_DATA_PLATE, BLOCK_TEMPLE_REFRACTORY_FIXTURE,
};

pub const WORLD_BLOCK_STONE: BlockDefinitionId = BlockDefinitionId::new(2);
pub const WORLD_BLOCK_TEMPLE_ARCHIVE: BlockDefinitionId = BlockDefinitionId::new(3);
pub const WORLD_BLOCK_TEMPLE_DRIVE: BlockDefinitionId = BlockDefinitionId::new(4);
pub const WORLD_BLOCK_TEMPLE_SEAL: BlockDefinitionId = BlockDefinitionId::new(5);
pub const WORLD_BLOCK_TEMPLE_TIN_DATA_PLATE: BlockDefinitionId = BlockDefinitionId::new(6);
pub const WORLD_BLOCK_TEMPLE_IRON_BEARING_HOUSING: BlockDefinitionId = BlockDefinitionId::new(7);
pub const WORLD_BLOCK_TEMPLE_REFRACTORY_FIXTURE: BlockDefinitionId = BlockDefinitionId::new(8);
pub const WORLD_BLOCK_TEMPLE_ZINC_CORROSION_PANEL: BlockDefinitionId = BlockDefinitionId::new(9);

pub const BLOCK_STATE_STONE: BlockStateId = BlockStateId::new(2);
pub const BLOCK_STATE_TEMPLE_ARCHIVE: BlockStateId = BlockStateId::new(3);
pub const BLOCK_STATE_TEMPLE_DRIVE: BlockStateId = BlockStateId::new(4);
pub const BLOCK_STATE_TEMPLE_SEAL: BlockStateId = BlockStateId::new(5);
pub const BLOCK_STATE_TEMPLE_TIN_DATA_PLATE: BlockStateId = BlockStateId::new(6);
pub const BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING: BlockStateId = BlockStateId::new(7);
pub const BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE: BlockStateId = BlockStateId::new(8);
pub const BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL: BlockStateId = BlockStateId::new(9);

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
