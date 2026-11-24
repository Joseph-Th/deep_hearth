//! Free-flying first-person camera and view-frustum culling.

use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_to_mat4;
use glam::{Mat4, Vec3, Vec4};

pub const NEAR: f32 = 0.08;
pub const FAR: f32 = 640.0;
const VERTICAL_FOV_DEGREES: f32 = 70.0;

pub struct Camera {
    pub position: Vec3,
    /// Heading in radians; zero looks toward +Z.
    pub yaw: f32,
    pub pitch: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }

    /// Horizontal forward and right vectors for walking-style movement.
    pub fn ground_axes(&self) -> (Vec3, Vec3) {
        let forward = Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos());
        (forward, forward.cross(Vec3::Y).normalize())
    }

    pub fn look(&mut self, delta_x: f32, delta_y: f32) {
        self.yaw -= delta_x * 0.0025;
        self.pitch = (self.pitch - delta_y * 0.0025).clamp(-1.55, 1.55);
    }

    /// Right-handed view with WebGPU 0..1 depth.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let view = look_to_mat4(self.position, self.forward(), Vec3::Y);
        let projection = perspective(VERTICAL_FOV_DEGREES.to_radians(), aspect, NEAR, FAR);
        projection * view
    }
}

/// Six inward-facing planes extracted from a clip matrix (WebGPU 0..1 depth).
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn new(view_projection: Mat4) -> Self {
        let rows = [
            view_projection.row(0),
            view_projection.row(1),
            view_projection.row(2),
            view_projection.row(3),
        ];
        Self {
            planes: [
                rows[3] + rows[0],
                rows[3] - rows[0],
                rows[3] + rows[1],
                rows[3] - rows[1],
                rows[2],
                rows[3] - rows[2],
            ],
        }
    }

    /// Conservative box test: false only when the box is wholly outside one plane.
    pub fn intersects(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        self.planes.iter().all(|plane| {
            let corner = Vec3::new(
                if plane.x >= 0.0 { max[0] } else { min[0] },
                if plane.y >= 0.0 { max[1] } else { min[1] },
                if plane.z >= 0.0 { max[2] } else { min[2] },
            );
            plane.truncate().dot(corner) + plane.w >= 0.0
        })
    }
}
