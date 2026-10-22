//! Sparse seeded temple precincts: excavatable architecture in one chunk per district.

use crate::content::{BLOCK_STATE_TEMPLE_ARCHIVE, BLOCK_STATE_TEMPLE_DRIVE};
use crate::spatial::VoxelCoord;
use crate::world::BlockStateId;

use super::{ChunkCoord, WorldGenerationSeed, mix64};

const DISTRICT_SIDE_CHUNKS: i64 = 8;
const CHAMBER_Y: i64 = -19;
const CHAMBER_RADIUS: u64 = 3;
pub(in crate::world) const STAIR_PATH: [(i64, i64); 15] = [
    (4, 3),
    (4, 4),
    (3, 4),
    (2, 4),
    (1, 4),
    (0, 4),
    (-1, 4),
    (-2, 4),
    (-3, 4),
    (-4, 4),
    (-4, 3),
    (-4, 2),
    (-4, 1),
    (-4, 0),
    (-4, -1),
];

#[derive(Clone, Copy, Debug)]
pub(in crate::world) struct TempleSite {
    center: VoxelCoord,
}

impl TempleSite {
    pub(in crate::world) fn for_chunk(
        seed: WorldGenerationSeed,
        chunk: ChunkCoord,
    ) -> Option<Self> {
        let district_x = chunk.x().div_euclid(DISTRICT_SIDE_CHUNKS);
        let district_z = chunk.z().div_euclid(DISTRICT_SIDE_CHUNKS);
        let hash = mix64(
            seed.value()
                ^ (district_x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (district_z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
                ^ 0x5445_4D50_4C45_4445,
        );
        // The district contains one site, not a free relic on every world's
        // starting (0,0) chunk. Euclidean division makes negative districts
        // symmetric and independent of chunk demand order.
        if chunk.x().rem_euclid(DISTRICT_SIDE_CHUNKS) != (hash % DISTRICT_SIDE_CHUNKS as u64) as i64
            || chunk.z().rem_euclid(DISTRICT_SIDE_CHUNKS)
                != ((hash >> 8) % DISTRICT_SIDE_CHUNKS as u64) as i64
        {
            return None;
        }
        let room_hash = mix64(hash ^ 0x5741_5244_454E_5353);
        let x = chunk
            .world_min_x()?
            .checked_add(6 + (room_hash % 5) as i64)?;
        let z = chunk
            .world_min_z()?
            .checked_add(6 + ((room_hash >> 8) % 5) as i64)?;
        Some(Self {
            center: VoxelCoord::new(x, CHAMBER_Y, z),
        })
    }

    pub(in crate::world) fn warden(self) -> VoxelCoord {
        VoxelCoord::new(self.center.x(), CHAMBER_Y, self.center.z() + 2)
    }

    pub(in crate::world) fn warden_route(self) -> [VoxelCoord; 3] {
        // One real hall cell per step; the archivist remains deeper in the sanctuary.
        [
            self.warden(),
            VoxelCoord::new(self.center.x(), CHAMBER_Y, self.center.z() + 1),
            self.center,
        ]
    }

    pub(in crate::world) fn archivist(self) -> VoxelCoord {
        VoxelCoord::new(self.center.x(), CHAMBER_Y, self.center.z() - 2)
    }

    /// A conspicuous copper cap over the last buried stair, not a free open shaft.
    pub(super) fn surface_seal(self, surface: i64, position: VoxelCoord) -> bool {
        position.x() == self.center.x() - 4
            && position.z() == self.center.z() - 1
            && position.y() == surface
    }

    fn stair_at(self, position: VoxelCoord) -> bool {
        STAIR_PATH.iter().enumerate().any(|(step, &(dx, dz))| {
            let foot = CHAMBER_Y
                + i64::try_from(step)
                    .unwrap_or_else(|_| unreachable!("bounded stair index fits i64"))
                + 1;
            position.x() == self.center.x() + dx
                && position.z() == self.center.z() + dz
                && (foot..=foot + 1).contains(&position.y())
        })
    }

    /// Some(AIR) is a real excavated-in-generation void; None leaves native host rock.
    pub(super) fn generated_state(self, position: VoxelCoord) -> Option<BlockStateId> {
        if self.stair_at(position) {
            return Some(BlockStateId::AIR);
        }
        if !(CHAMBER_Y..=CHAMBER_Y + 2).contains(&position.y()) {
            return None;
        }
        let dx = self.center.x().abs_diff(position.x());
        let dz = self.center.z().abs_diff(position.z());
        // West is the archive, east the engine bay, both with real connecting throats.
        let main_hall = dx <= CHAMBER_RADIUS && dz <= CHAMBER_RADIUS;
        let annex = (4..=5).contains(&dx) && dz <= 1;
        if !(main_hall || annex) {
            return None;
        }
        let base = position.y() == CHAMBER_Y;
        if base && position.x() == self.center.x() - 5 && position.z() == self.center.z() {
            return Some(BLOCK_STATE_TEMPLE_ARCHIVE);
        }
        if base && position.x() == self.center.x() + 5 && position.z() == self.center.z() {
            return Some(BLOCK_STATE_TEMPLE_DRIVE);
        }
        Some(BlockStateId::AIR)
    }
}
