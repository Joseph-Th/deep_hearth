//! Rare seeded deep temple vaults, sealed into real persistent world rock.

use crate::content::{BLOCK_STATE_TEMPLE_ARCHIVE, BLOCK_STATE_TEMPLE_DRIVE};
use crate::spatial::VoxelCoord;
use crate::world::BlockStateId;

use super::{ChunkCoord, WorldGenerationSeed, mix64};

const DISTRICT_SIDE_CHUNKS: i64 = 8;
const CHAMBER_Y: i64 = -19;
const CHAMBER_RADIUS: u64 = 3;

#[derive(Clone, Copy, Debug)]
pub(in crate::world) struct TempleSite {
    center: VoxelCoord,
}

impl TempleSite {
    pub(in crate::world) fn for_chunk(
        seed: WorldGenerationSeed,
        chunk: ChunkCoord,
    ) -> Option<Self> {
        if chunk.x().rem_euclid(DISTRICT_SIDE_CHUNKS) != 0
            || chunk.z().rem_euclid(DISTRICT_SIDE_CHUNKS) != 0
        {
            return None;
        }
        let hash = mix64(
            seed.value()
                ^ (chunk.x() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (chunk.z() as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
                ^ 0x5445_4D50_4C45_4445,
        );
        let x = chunk.world_min_x()?.checked_add(6 + (hash % 5) as i64)?;
        let z = chunk
            .world_min_z()?
            .checked_add(6 + ((hash >> 8) % 5) as i64)?;
        Some(Self {
            center: VoxelCoord::new(x, CHAMBER_Y, z),
        })
    }

    pub(in crate::world) fn warden(self) -> VoxelCoord {
        VoxelCoord::new(self.center.x(), CHAMBER_Y, self.center.z() + 2)
    }

    pub(in crate::world) fn archivist(self) -> VoxelCoord {
        VoxelCoord::new(self.center.x(), CHAMBER_Y, self.center.z() - 2)
    }

    /// Some(AIR) is a real excavated-in-generation void; None leaves native host rock.
    pub(super) fn generated_state(self, position: VoxelCoord) -> Option<BlockStateId> {
        if self.center.x().abs_diff(position.x()) > CHAMBER_RADIUS
            || self.center.z().abs_diff(position.z()) > CHAMBER_RADIUS
            || !(CHAMBER_Y..=CHAMBER_Y + 2).contains(&position.y())
        {
            return None;
        }
        if position.y() == CHAMBER_Y {
            if position.x() == self.center.x() - 2 && position.z() == self.center.z() {
                return Some(BLOCK_STATE_TEMPLE_ARCHIVE);
            }
            if position.x() == self.center.x() + 2 && position.z() == self.center.z() {
                return Some(BLOCK_STATE_TEMPLE_DRIVE);
            }
        }
        Some(BlockStateId::AIR)
    }
}
