//! Owns built-in block and block-state content for persistent terrain.

use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::material::{CommodityKey, MaterialId, MaterialRegistry};
use crate::world::{
    BlockCollisionShape, BlockDefinition, BlockDefinitionId, BlockExcavationDefinition,
    BlockPhysicalDefinition, BlockStateDefinition, BlockStateId, WorldRegistry,
};

use super::materials::{FORM_BLOCK, FORM_CHIP, FORM_SCRAP, MATERIAL_COPPER, MATERIAL_STONE};
use super::textures::{BLOCK_COPPER, BLOCK_STONE};

pub const WORLD_BLOCK_STONE: BlockDefinitionId = BlockDefinitionId::new(2);
pub const WORLD_BLOCK_TEMPLE_ARCHIVE: BlockDefinitionId = BlockDefinitionId::new(3);
pub const WORLD_BLOCK_TEMPLE_DRIVE: BlockDefinitionId = BlockDefinitionId::new(4);
pub const WORLD_BLOCK_TEMPLE_SEAL: BlockDefinitionId = BlockDefinitionId::new(5);

pub const BLOCK_STATE_STONE: BlockStateId = BlockStateId::new(2);
pub const BLOCK_STATE_TEMPLE_ARCHIVE: BlockStateId = BlockStateId::new(3);
pub const BLOCK_STATE_TEMPLE_DRIVE: BlockStateId = BlockStateId::new(4);
pub const BLOCK_STATE_TEMPLE_SEAL: BlockStateId = BlockStateId::new(5);

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
        ],
        [
            BlockStateDefinition::new(BlockStateId::AIR, BlockDefinitionId::AIR),
            BlockStateDefinition::new(BLOCK_STATE_STONE, WORLD_BLOCK_STONE),
            BlockStateDefinition::new(BLOCK_STATE_TEMPLE_ARCHIVE, WORLD_BLOCK_TEMPLE_ARCHIVE),
            BlockStateDefinition::new(BLOCK_STATE_TEMPLE_DRIVE, WORLD_BLOCK_TEMPLE_DRIVE),
            BlockStateDefinition::new(BLOCK_STATE_TEMPLE_SEAL, WORLD_BLOCK_TEMPLE_SEAL),
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
