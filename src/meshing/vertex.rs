//! Surface-shader vertex encoding shared by terrain and vegetation meshes.

/// Bytes per encoded [`SurfaceVertex`], matching the surface vertex buffer layout.
pub const SURFACE_VERTEX_STRIDE: usize = 64;

/// Byte offsets of surface vertex locations 0 through 4 inside one encoded vertex.
pub const SURFACE_VERTEX_OFFSETS: [usize; 5] = [0, 12, 28, 44, 48];

/// One vertex in the surface-shader layout documented by `assets/shaders/README.md`.
///
/// Positions are relative to the caller's integer render origin so large world coordinates keep
/// full `f32` precision near the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceVertex {
    position: [f32; 3],
    uv_light: [f32; 4],
    normal_ao: [f32; 4],
    texture_key: u32,
    tint: [f32; 4],
}

impl SurfaceVertex {
    #[must_use]
    pub(super) const fn new(
        position: [f32; 3],
        uv: [f32; 2],
        light: [f32; 2],
        normal: [f32; 3],
        ambient_occlusion: f32,
        texture_key: u32,
        tint: [f32; 4],
    ) -> Self {
        Self {
            position,
            uv_light: [uv[0], uv[1], light[0], light[1]],
            normal_ao: [normal[0], normal[1], normal[2], ambient_occlusion],
            texture_key,
            tint,
        }
    }

    #[must_use]
    pub const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Texture UV in block units; the shader repeats each whole unit.
    #[must_use]
    pub const fn uv(&self) -> [f32; 2] {
        [self.uv_light[0], self.uv_light[1]]
    }

    /// Normalized sky and block light.
    #[must_use]
    pub const fn light(&self) -> [f32; 2] {
        [self.uv_light[2], self.uv_light[3]]
    }

    #[must_use]
    pub const fn normal(&self) -> [f32; 3] {
        [self.normal_ao[0], self.normal_ao[1], self.normal_ao[2]]
    }

    #[must_use]
    pub const fn ambient_occlusion(&self) -> f32 {
        self.normal_ao[3]
    }

    #[must_use]
    pub const fn texture_key(&self) -> u32 {
        self.texture_key
    }

    #[must_use]
    pub const fn tint(&self) -> [f32; 4] {
        self.tint
    }

    fn write_le_bytes(&self, out: &mut Vec<u8>) {
        let floats = self
            .position
            .iter()
            .chain(&self.uv_light)
            .chain(&self.normal_ao);
        for value in floats {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&self.texture_key.to_le_bytes());
        for value in &self.tint {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
}

/// Indexed triangle list for one surface pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceMesh {
    vertices: Vec<SurfaceVertex>,
    indices: Vec<u32>,
}

impl SurfaceMesh {
    #[must_use]
    pub fn vertices(&self) -> &[SurfaceVertex] {
        &self.vertices
    }

    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Little-endian vertex upload bytes in [`SURFACE_VERTEX_STRIDE`] records.
    #[must_use]
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vertices.len() * SURFACE_VERTEX_STRIDE);
        for vertex in &self.vertices {
            vertex.write_le_bytes(&mut bytes);
        }
        bytes
    }

    /// Little-endian `u32` index upload bytes.
    #[must_use]
    pub fn index_bytes(&self) -> Vec<u8> {
        self.indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect()
    }

    /// Appends one counter-clockwise quad. `split_odd` triangulates across corners 1 and 3,
    /// which keeps anisotropic corner occlusion from creasing along the wrong diagonal.
    pub(super) fn push_quad(&mut self, corners: [SurfaceVertex; 4], split_odd: bool) {
        let base = u32::try_from(self.vertices.len())
            .unwrap_or_else(|_| unreachable!("one bounded mesh cannot exceed u32 vertices"));
        self.vertices.extend_from_slice(&corners);
        let order: [u32; 6] = if split_odd {
            [1, 2, 3, 1, 3, 0]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        self.indices
            .extend(order.iter().map(|offset| base + offset));
    }
}

/// Opaque and alpha-tested geometry; the passes differ in culling and shadow routing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SectionMesh {
    opaque: SurfaceMesh,
    cutout: SurfaceMesh,
}

impl SectionMesh {
    #[must_use]
    pub const fn opaque(&self) -> &SurfaceMesh {
        &self.opaque
    }

    /// Alpha-tested, double-sided geometry that casts shadows through the cutout pass.
    #[must_use]
    pub const fn cutout(&self) -> &SurfaceMesh {
        &self.cutout
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.opaque.is_empty() && self.cutout.is_empty()
    }

    pub(super) fn pass_mut(&mut self, cutout: bool) -> &mut SurfaceMesh {
        if cutout {
            &mut self.cutout
        } else {
            &mut self.opaque
        }
    }
}
