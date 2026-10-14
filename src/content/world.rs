//! Owns built-in block and block-state content for persistent terrain.

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialId, MaterialRegistry};
use crate::world::{
    BlockCollisionShape, BlockDefinition, BlockDefinitionId, BlockPhysicalDefinition,
    BlockStateDefinition, BlockStateId, WorldRegistry,
};

use super::materials::{FORM_BLOCK, MATERIAL_STONE};
use super::textures::BLOCK_STONE;

pub const WORLD_BLOCK_STONE: BlockDefinitionId = BlockDefinitionId::new(2);

pub const BLOCK_STATE_STONE: BlockStateId = BlockStateId::new(2);

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
                ),
                Some(BLOCK_STONE),
            ),
        ],
        [
            BlockStateDefinition::new(BlockStateId::AIR, BlockDefinitionId::AIR),
            BlockStateDefinition::new(BLOCK_STATE_STONE, WORLD_BLOCK_STONE),
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
