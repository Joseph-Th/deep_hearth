//! Living-tree presentation: physical trunks plus seasonal, species-shaped crowns.
//!
//! Crowns are visual only. The world owns trunk collision and canopy shade; this module derives a
//! deterministic leaf volume from each tree's species, current canopy radius and seed origin.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::{Season, SimulationTick};
use crate::spatial::VoxelCoord;
use crate::texture::BakedBlockAppearance;
use crate::world::{ChunkCoord, TreeRecord, TreeSpecies, WorldState};

use super::SectionMesher;
use super::quad::{CornerShade, FaceAxes, Quad};
use super::vertex::{SectionMesh, SurfaceMesh};

const NEUTRAL_TINT: [f32; 4] = [1.0; 4];
const HALF_CHUNK: i64 = 8;

/// Linear crown tint per species: conifers are darker and bluer, birch and willow paler.
const fn species_tint(species: TreeSpecies) -> [f32; 3] {
    match species {
        TreeSpecies::Alder => [0.80, 0.95, 0.75],
        TreeSpecies::Willow => [0.95, 1.05, 0.70],
        TreeSpecies::Oak => [0.78, 0.90, 0.68],
        TreeSpecies::Birch => [1.00, 1.08, 0.72],
        TreeSpecies::Pinyon => [0.55, 0.75, 0.62],
        TreeSpecies::Juniper => [0.60, 0.78, 0.70],
        TreeSpecies::Pioneer => [0.95, 1.05, 0.72],
    }
}

const AUTUMN_TINT: [f32; 3] = [1.25, 0.82, 0.42];

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn cell_hash(seed: u64, cell: VoxelCoord) -> u64 {
    mix64(
        seed ^ (cell.x() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (cell.y() as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (cell.z() as u64).wrapping_mul(0x1656_67B1_9E37_79F9),
    )
}

struct TreeForm {
    root: VoxelCoord,
    radius: i64,
    conifer: bool,
    seed: u64,
    tint: [f32; 4],
}

impl TreeForm {
    fn new(tree: TreeRecord, at: SimulationTick, season: Season) -> Self {
        let species = tree.species();
        let origin = tree.origin_root();
        let seed =
            mix64((origin.x() as u64).wrapping_mul(0xD1B5_4A32_D192_ED03) ^ (origin.z() as u64));
        let base = if season == Season::Autumn && !species.evergreen() {
            AUTUMN_TINT
        } else {
            species_tint(species)
        };
        // A few percent of per-tree variation keeps a stand from reading as one repeated asset.
        let jitter = 0.94 + (seed % 13) as f32 * 0.01;
        Self {
            root: tree.root(),
            radius: i64::from(tree.canopy_radius_voxels(at)),
            conifer: species.evergreen(),
            seed,
            tint: [base[0] * jitter, base[1] * jitter, base[2] * jitter, 1.0],
        }
    }

    /// Physical trunks occupy two cells; older trees show more bole beneath the crown.
    fn trunk_height(&self) -> i64 {
        2 + self.radius
    }

    fn trunk_cells(&self, out: &mut BTreeSet<VoxelCoord>) {
        for dy in 0..self.trunk_height() {
            if let Some(cell) = self.at(0, dy, 0) {
                out.insert(cell);
            }
        }
    }

    fn at(&self, dx: i64, dy: i64, dz: i64) -> Option<VoxelCoord> {
        offset(self.root, [dx, dy, dz])
    }

    fn crown_cells(&self, out: &mut BTreeMap<VoxelCoord, [f32; 4]>) {
        if self.radius == 0 {
            if let Some(cell) = self.at(0, self.trunk_height(), 0) {
                out.entry(cell).or_insert(self.tint);
            }
        } else if self.conifer {
            self.conifer_crown(out);
        } else {
            self.broadleaf_crown(out);
        }
    }

    /// Ellipsoid crown around the top of the bole, with a ragged hashed rim.
    fn broadleaf_crown(&self, out: &mut BTreeMap<VoxelCoord, [f32; 4]>) {
        let horizontal = self.radius as f32 + 0.5;
        let vertical = self.radius as f32 * 0.7 + 0.8;
        let center_y = self.trunk_height() - 1;
        let reach = vertical.ceil() as i64;
        for dy in -reach..=reach {
            for dz in -self.radius..=self.radius {
                for dx in -self.radius..=self.radius {
                    let extent = ((dx * dx + dz * dz) as f32) / (horizontal * horizontal)
                        + ((dy * dy) as f32) / (vertical * vertical);
                    let Some(cell) = self.at(dx, center_y + dy, dz) else {
                        continue;
                    };
                    let ragged = extent > 0.55 && cell_hash(self.seed, cell).is_multiple_of(4);
                    if extent <= 1.0 && !ragged {
                        out.entry(cell).or_insert(self.tint);
                    }
                }
            }
        }
    }

    /// Stacked tiers narrowing to a single leader cell.
    fn conifer_crown(&self, out: &mut BTreeMap<VoxelCoord, [f32; 4]>) {
        let height = self.trunk_height();
        for dy in 1..=height {
            let taper = 1.0 - (dy - 1) as f32 / height as f32;
            let tier = (self.radius as f32 * taper + 0.35).round() as i64;
            for dz in -tier..=tier {
                for dx in -tier..=tier {
                    let Some(cell) = self.at(dx, dy, dz) else {
                        continue;
                    };
                    let corner = dx.abs() == tier && dz.abs() == tier && tier > 0;
                    if !corner || cell_hash(self.seed, cell).is_multiple_of(3) {
                        out.entry(cell).or_insert(self.tint);
                    }
                }
            }
        }
        if let Some(cell) = self.at(0, height + 1, 0) {
            out.entry(cell).or_insert(self.tint);
        }
    }
}

fn offset(cell: VoxelCoord, [dx, dy, dz]: [i64; 3]) -> Option<VoxelCoord> {
    Some(VoxelCoord::new(
        cell.x().checked_add(dx)?,
        cell.y().checked_add(dy)?,
        cell.z().checked_add(dz)?,
    ))
}

pub(super) fn mesh_chunk_trees(
    mesher: &SectionMesher<'_>,
    world: &WorldState,
    chunk: ChunkCoord,
    at: SimulationTick,
    origin: VoxelCoord,
) -> SectionMesh {
    let mut mesh = SectionMesh::default();
    let (Some(trunk), Some(foliage), Some(min_x), Some(min_z)) = (
        mesher.trunk,
        mesher.foliage,
        chunk.world_min_x(),
        chunk.world_min_z(),
    ) else {
        return mesh;
    };
    let season = mesher.calendar.date_at(at).season();
    let center = VoxelCoord::new(min_x + HALF_CHUNK, 0, min_z + HALF_CHUNK);
    let mut trunks = BTreeSet::new();
    let mut crowns = BTreeMap::new();
    for tree in world
        .trees_near_horizontal(center, HALF_CHUNK)
        .filter(|tree| ChunkCoord::containing(tree.root()) == chunk)
    {
        let form = TreeForm::new(*tree, at, season);
        form.trunk_cells(&mut trunks);
        // Deciduous crowns are bare in winter, matching their lost canopy shade.
        if form.conifer || season != Season::Winter {
            form.crown_cells(&mut crowns);
        }
    }
    let terrain_opaque = |cell: VoxelCoord| {
        world
            .block_at(cell)
            .is_some_and(|state| mesher.looks.is_opaque(state))
    };
    trunks.retain(|cell| !terrain_opaque(*cell));
    crowns.retain(|cell, _| !trunks.contains(cell) && !terrain_opaque(*cell));
    for &cell in &trunks {
        emit_cube(
            mesh.pass_mut(false),
            trunk,
            cell,
            NEUTRAL_TINT,
            origin,
            |neighbor| trunks.contains(&neighbor) || terrain_opaque(neighbor),
        );
    }
    for (&cell, &tint) in &crowns {
        emit_cube(
            mesh.pass_mut(true),
            foliage,
            cell,
            tint,
            origin,
            |neighbor| {
                crowns.contains_key(&neighbor)
                    || trunks.contains(&neighbor)
                    || terrain_opaque(neighbor)
            },
        );
    }
    mesh
}

fn emit_cube(
    mesh: &mut SurfaceMesh,
    appearance: BakedBlockAppearance,
    cell: VoxelCoord,
    tint: [f32; 4],
    origin: VoxelCoord,
    hidden: impl Fn(VoxelCoord) -> bool,
) {
    let local = [
        (i128::from(cell.x()) - i128::from(origin.x())) as f64,
        (i128::from(cell.y()) - i128::from(origin.y())) as f64,
        (i128::from(cell.z()) - i128::from(origin.z())) as f64,
    ];
    for axes in FaceAxes::ALL {
        if offset(cell, axes.normal()).is_some_and(&hidden) {
            continue;
        }
        Quad {
            axes,
            cell: local,
            width: 1,
            height: 1,
            texture_key: appearance.texture(axes.face).gpu_key(),
            shade: CornerShade::OPEN_SKY,
            tint,
        }
        .emit(mesh);
    }
}
