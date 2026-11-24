//! Axis-aligned face geometry shared by greedy terrain and vegetation emission.

use crate::spatial::CubeFace;

use super::vertex::{SurfaceMesh, SurfaceVertex};

/// Darkening applied for 0..=3 occluding neighbors around one face corner.
const AMBIENT_OCCLUSION_LEVELS: [f32; 4] = [1.0, 0.78, 0.6, 0.42];

/// Corner light is a 0..=60 average of four 0..=15 sky samples.
pub(super) const CORNER_LIGHT_SCALE: f32 = 60.0;

/// Face orientation: normal axis, sign, and the two tangent axes with `a x b = +normal axis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FaceAxes {
    pub(super) face: CubeFace,
    pub(super) normal_axis: usize,
    pub(super) positive: bool,
    pub(super) a: usize,
    pub(super) b: usize,
}

impl FaceAxes {
    pub(super) const ALL: [Self; 6] = [
        Self::new(CubeFace::East, 0, true),
        Self::new(CubeFace::West, 0, false),
        Self::new(CubeFace::Top, 1, true),
        Self::new(CubeFace::Bottom, 1, false),
        Self::new(CubeFace::South, 2, true),
        Self::new(CubeFace::North, 2, false),
    ];

    const fn new(face: CubeFace, normal_axis: usize, positive: bool) -> Self {
        Self {
            face,
            normal_axis,
            positive,
            a: (normal_axis + 1) % 3,
            b: (normal_axis + 2) % 3,
        }
    }

    pub(super) const fn normal(self) -> [i64; 3] {
        let mut normal = [0; 3];
        normal[self.normal_axis] = if self.positive { 1 } else { -1 };
        normal
    }

    /// Corner tangent signs in canonical order (0,0), (1,0), (1,1), (0,1) over (a, b).
    pub(super) const fn corner_offsets(self, corner: usize) -> ([i64; 3], [i64; 3]) {
        let (along_a, along_b) = match corner {
            0 => (-1, -1),
            1 => (1, -1),
            2 => (1, 1),
            _ => (-1, 1),
        };
        let mut a = [0; 3];
        let mut b = [0; 3];
        a[self.a] = along_a;
        b[self.b] = along_b;
        (a, b)
    }
}

/// Per-corner shading for one emitted rectangle, in canonical corner order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CornerShade {
    pub(super) occlusion: [u8; 4],
    pub(super) light: [u8; 4],
}

impl CornerShade {
    pub(super) const OPEN_SKY: Self = Self {
        occlusion: [0; 4],
        light: [60; 4],
    };

    pub(super) fn is_uniform(self) -> bool {
        self.occlusion
            .iter()
            .all(|&value| value == self.occlusion[0])
            && self.light.iter().all(|&value| value == self.light[0])
    }

    fn brightness(self, corner: usize) -> u32 {
        u32::from(3 - self.occlusion[corner]) * 64 + u32::from(self.light[corner])
    }
}

/// A `width` x `height` rectangle of faces whose cell-space minimum corner is `cell`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Quad {
    pub(super) axes: FaceAxes,
    /// Origin-relative cell coordinate of the rectangle's minimum cell.
    pub(super) cell: [f64; 3],
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) texture_key: u32,
    pub(super) shade: CornerShade,
    pub(super) tint: [f32; 4],
}

impl Quad {
    pub(super) fn emit(self, mesh: &mut SurfaceMesh) {
        let axes = self.axes;
        let mut normal = [0.0_f32; 3];
        normal[axes.normal_axis] = if axes.positive { 1.0 } else { -1.0 };
        let corners: [SurfaceVertex; 4] = std::array::from_fn(|corner| {
            let (along_a, along_b) = match corner {
                0 => (0.0, 0.0),
                1 => (1.0, 0.0),
                2 => (1.0, 1.0),
                _ => (0.0, 1.0),
            };
            let mut position = self.cell;
            if axes.positive {
                position[axes.normal_axis] += 1.0;
            }
            position[axes.a] += along_a * f64::from(self.width);
            position[axes.b] += along_b * f64::from(self.height);
            let light = f32::from(self.shade.light[corner]) / CORNER_LIGHT_SCALE;
            SurfaceVertex::new(
                position.map(|value| value as f32),
                face_uv(axes, position),
                [light, 0.0],
                normal,
                AMBIENT_OCCLUSION_LEVELS[usize::from(self.shade.occlusion[corner])],
                self.texture_key,
                self.tint,
            )
        });
        // Canonical corners wind counter-clockwise around +normal; negative faces reverse.
        let order: [usize; 4] = if axes.positive {
            [0, 1, 2, 3]
        } else {
            [0, 3, 2, 1]
        };
        let oriented = order.map(|corner| corners[corner]);
        let bright = order.map(|corner| self.shade.brightness(corner));
        let split_odd = bright[0] + bright[2] < bright[1] + bright[3];
        mesh.push_quad(oriented, split_odd);
    }
}

/// World-aligned UVs: horizontal faces tile in X/Z; vertical faces keep texture "up" as +Y.
fn face_uv(axes: FaceAxes, position: [f64; 3]) -> [f32; 2] {
    let [x, y, z] = position;
    let uv = match axes.normal_axis {
        0 => [z, -y],
        1 => [x, z],
        _ => [x, -y],
    };
    uv.map(|value| value as f32)
}
