//! Greedy terrain-section meshing with per-corner occlusion and skylight.

use crate::spatial::VoxelCoord;
use crate::texture::TextureAlphaMode;
use crate::world::{ChunkCoord, SECTION_SIDE, WORLD_MIN_Y, WorldState};

use super::StateLooks;
use super::quad::{CornerShade, FaceAxes, Quad};
use super::vertex::SectionMesh;
use super::volume::{PaddedVolume, SIDE};

const SIDE_I64: i64 = SIDE as i64;
const NEUTRAL_TINT: [f32; 4] = [1.0; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FaceKey {
    texture_key: u32,
    cutout: bool,
    shade: CornerShade,
}

type FaceMask = [Option<FaceKey>; SIDE * SIDE];

pub(super) fn mesh_section(
    looks: &StateLooks<'_>,
    world: &WorldState,
    chunk: ChunkCoord,
    section_y: i64,
    origin: VoxelCoord,
) -> SectionMesh {
    let mut mesh = SectionMesh::default();
    let (Some(min_x), Some(min_z), Some(min_y)) = (
        chunk.world_min_x(),
        chunk.world_min_z(),
        section_y.checked_mul(SIDE_I64),
    ) else {
        return mesh;
    };
    if !section_has_blocks(world, chunk, section_y) || is_buried(looks, world, chunk, section_y) {
        return mesh;
    }
    let volume = PaddedVolume::gather(world, looks, [min_x, min_y, min_z]);
    let base = [
        relative(min_x, origin.x()),
        relative(min_y, origin.y()),
        relative(min_z, origin.z()),
    ];
    for axes in FaceAxes::ALL {
        for slice in 0..SIDE_I64 {
            let mut mask = face_mask(looks, &volume, axes, slice);
            emit_greedy(&mut mesh, axes, slice, &mut mask, base);
        }
    }
    mesh
}

/// Exact `i64` difference widened before conversion, so no coordinate pair can overflow.
fn relative(value: i64, origin: i64) -> f64 {
    (i128::from(value) - i128::from(origin)) as f64
}

fn section_has_blocks(world: &WorldState, chunk: ChunkCoord, section_y: i64) -> bool {
    world.get_chunk(chunk).is_some_and(|column| {
        column
            .sections()
            .any(|section| section.y() == section_y && section.non_air_count() != 0)
    })
}

/// A wholly opaque section enclosed by wholly opaque neighbors has no visible face.
fn is_buried(
    looks: &StateLooks<'_>,
    world: &WorldState,
    chunk: ChunkCoord,
    section_y: i64,
) -> bool {
    let below_floor = (section_y - 1) * SIDE_I64 + SIDE_I64 <= WORLD_MIN_Y;
    let solid = |coord: ChunkCoord, y: i64| section_is_opaque(looks, world, coord, y);
    solid(chunk, section_y)
        && solid(chunk, section_y + 1)
        && (below_floor || solid(chunk, section_y - 1))
        && solid(ChunkCoord::new(chunk.x() + 1, chunk.z()), section_y)
        && solid(ChunkCoord::new(chunk.x() - 1, chunk.z()), section_y)
        && solid(ChunkCoord::new(chunk.x(), chunk.z() + 1), section_y)
        && solid(ChunkCoord::new(chunk.x(), chunk.z() - 1), section_y)
}

fn section_is_opaque(
    looks: &StateLooks<'_>,
    world: &WorldState,
    chunk: ChunkCoord,
    section_y: i64,
) -> bool {
    world.get_chunk(chunk).is_some_and(|column| {
        column.sections().any(|section| {
            section.y() == section_y
                && usize::from(section.non_air_count()) == SECTION_SIDE.pow(3)
                && section
                    .palette()
                    .iter()
                    .all(|&state| looks.is_opaque(state))
        })
    })
}

fn cell_at(axes: FaceAxes, slice: i64, i: usize, j: usize) -> [i64; 3] {
    let mut cell = [0; 3];
    cell[axes.normal_axis] = slice;
    cell[axes.a] = i as i64;
    cell[axes.b] = j as i64;
    cell
}

fn offset(cell: [i64; 3], by: [i64; 3]) -> [i64; 3] {
    [cell[0] + by[0], cell[1] + by[1], cell[2] + by[2]]
}

fn face_mask(
    looks: &StateLooks<'_>,
    volume: &PaddedVolume,
    axes: FaceAxes,
    slice: i64,
) -> FaceMask {
    std::array::from_fn(|index| {
        let cell = cell_at(axes, slice, index % SIDE, index / SIDE);
        face_key(looks, volume, axes, cell)
    })
}

fn face_key(
    looks: &StateLooks<'_>,
    volume: &PaddedVolume,
    axes: FaceAxes,
    cell: [i64; 3],
) -> Option<FaceKey> {
    let state = volume.state(cell);
    let look = looks.look(state)?;
    let appearance = look.appearance?;
    let outside = offset(cell, axes.normal());
    if volume.is_opaque(outside) || (!look.opaque && volume.state(outside) == state) {
        return None;
    }
    let texture = appearance.texture(axes.face);
    Some(FaceKey {
        texture_key: texture.gpu_key(),
        cutout: texture.alpha_mode() != TextureAlphaMode::Opaque,
        shade: corner_shade(volume, axes, outside),
    })
}

/// Classic voxel corner occlusion plus smooth skylight from the four cells meeting at each
/// corner on the open side of the face. A corner pinched by both sides ignores its diagonal.
fn corner_shade(volume: &PaddedVolume, axes: FaceAxes, outside: [i64; 3]) -> CornerShade {
    let mut shade = CornerShade {
        occlusion: [0; 4],
        light: [0; 4],
    };
    for corner in 0..4 {
        let (along_a, along_b) = axes.corner_offsets(corner);
        let side_a = offset(outside, along_a);
        let side_b = offset(outside, along_b);
        let diagonal = offset(side_a, along_b);
        let (open_a, open_b) = (!volume.is_opaque(side_a), !volume.is_opaque(side_b));
        let diagonal_opaque = volume.is_opaque(diagonal);
        let open_diagonal = (open_a || open_b) && !diagonal_opaque;
        shade.occlusion[corner] = if open_a || open_b {
            u8::from(!open_a) + u8::from(!open_b) + u8::from(diagonal_opaque)
        } else {
            3
        };
        let samples = [
            (true, outside),
            (open_a, side_a),
            (open_b, side_b),
            (open_diagonal, diagonal),
        ];
        let (sum, count) = samples
            .iter()
            .filter(|(open, _)| *open)
            .fold((0_u32, 0_u32), |(sum, count), (_, cell)| {
                (sum + u32::from(volume.sky(*cell)), count + 1)
            });
        shade.light[corner] = u8::try_from(sum * 4 / count)
            .unwrap_or_else(|_| unreachable!("four sky levels average within u8"));
    }
    shade
}

fn emit_greedy(
    mesh: &mut SectionMesh,
    axes: FaceAxes,
    slice: i64,
    mask: &mut FaceMask,
    base: [f64; 3],
) {
    for j in 0..SIDE {
        let mut i = 0;
        while i < SIDE {
            let Some(key) = mask[i + j * SIDE] else {
                i += 1;
                continue;
            };
            let (width, height) = if key.shade.is_uniform() {
                merged_extent(mask, i, j, key)
            } else {
                (1, 1)
            };
            for row in j..j + height {
                mask[row * SIDE + i..row * SIDE + i + width].fill(None);
            }
            let local = cell_at(axes, slice, i, j);
            let cell = std::array::from_fn(|axis| base[axis] + local[axis] as f64);
            Quad {
                axes,
                cell,
                width: width as u32,
                height: height as u32,
                texture_key: key.texture_key,
                shade: key.shade,
                tint: NEUTRAL_TINT,
            }
            .emit(mesh.pass_mut(key.cutout));
            i += width;
        }
    }
}

fn merged_extent(mask: &FaceMask, i: usize, j: usize, key: FaceKey) -> (usize, usize) {
    let mut width = 1;
    while i + width < SIDE && mask[i + width + j * SIDE] == Some(key) {
        width += 1;
    }
    let mut height = 1;
    while j + height < SIDE
        && mask[(j + height) * SIDE + i..(j + height) * SIDE + i + width]
            .iter()
            .all(|&entry| entry == Some(key))
    {
        height += 1;
    }
    (width, height)
}
