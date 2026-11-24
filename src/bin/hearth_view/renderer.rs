//! GPU resources and the per-frame pass sequence from the shader contract:
//! directional shadow -> sky -> opaque/cutout surfaces -> tone-mapped display.
//!
//! Tiled point lights and bloom are bound to neutral resources (no lights, black bloom), which the
//! contract permits while their producers are disabled.

use glam::camera::rh::proj::directx::orthographic;
use glam::camera::rh::view::look_to_mat4;
use glam::{Mat4, Vec3};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

use deep_hearth::meshing::SurfaceMesh;
use deep_hearth::texture::BakedTextureArray;

use crate::camera::{Camera, FAR, Frustum};
use crate::daylight::Daylight;
use crate::pipelines::{DEPTH_FORMAT, HDR_FORMAT, Layouts, Pipelines};
use crate::scene::{Placed, Scene};

const SHADOW_SIZE: u32 = 2048;
/// Half-width of the square shadow volume centered near the camera, in blocks.
const SHADOW_RADIUS: f32 = 96.0;
const SHADOW_DEPTH: f32 = 600.0;

struct GpuMesh {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    count: u32,
}

struct Drawable {
    min: [f32; 3],
    max: [f32; 3],
    opaque: Option<GpuMesh>,
    cutout: Option<GpuMesh>,
}

struct Targets {
    width: u32,
    height: u32,
    hdr: wgpu::TextureView,
    depth: wgpu::TextureView,
    post_textures: wgpu::BindGroup,
}

struct FrameBuffers {
    surface: wgpu::Buffer,
    shadow: wgpu::Buffer,
    sky: wgpu::Buffer,
    post: wgpu::Buffer,
}

struct Groups {
    surface_frame: wgpu::BindGroup,
    textures: wgpu::BindGroup,
    lights: wgpu::BindGroup,
    shadow_frame: wgpu::BindGroup,
    sky_frame: wgpu::BindGroup,
    post_frame: wgpu::BindGroup,
}

pub struct FrameInput<'a> {
    pub camera: &'a Camera,
    pub daylight: &'a Daylight,
    pub time_seconds: f32,
    pub frame_index: u32,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    layouts: Layouts,
    pipelines: Pipelines,
    buffers: FrameBuffers,
    groups: Groups,
    shadow_view: wgpu::TextureView,
    bloom_view: wgpu::TextureView,
    linear_sampler: wgpu::Sampler,
    targets: Targets,
    terrain: Vec<Drawable>,
    trees: Vec<Drawable>,
}

fn upload(device: &wgpu::Device, mesh: &SurfaceMesh) -> Option<GpuMesh> {
    if mesh.is_empty() {
        return None;
    }
    Some(GpuMesh {
        vertex: device.create_buffer_init(&BufferInitDescriptor {
            label: Some("surface vertices"),
            contents: &mesh.vertex_bytes(),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        index: device.create_buffer_init(&BufferInitDescriptor {
            label: Some("surface indices"),
            contents: &mesh.index_bytes(),
            usage: wgpu::BufferUsages::INDEX,
        }),
        count: u32::try_from(mesh.indices().len()).unwrap_or(u32::MAX),
    })
}

fn drawables(device: &wgpu::Device, placed: &[Placed]) -> Vec<Drawable> {
    placed
        .iter()
        .map(|placed| Drawable {
            min: placed.min,
            max: placed.max,
            opaque: upload(device, placed.mesh.opaque()),
            cutout: upload(device, placed.mesh.cutout()),
        })
        .collect()
}

fn texture_2d(
    device: &wgpu::Device,
    label: &str,
    size: (u32, u32),
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn write_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mip: u32,
    layer_stride: u32,
    size: wgpu::Extent3d,
    bytes: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: mip,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(layer_stride),
            rows_per_image: Some(size.height),
        },
        size,
    );
}

/// Uploads the indexed texture array, palette-row lookup and palette colors without transcoding.
fn indexed_texture_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    baked: &BakedTextureArray,
) -> wgpu::BindGroup {
    let layers = u32::from(baked.pattern_layer_count()).max(1);
    let base = baked
        .mip_levels()
        .first()
        .map_or(1, |mip| u32::from(mip.side()));
    let indexed = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("indexed texture array"),
        size: wgpu::Extent3d {
            width: base,
            height: base,
            depth_or_array_layers: layers,
        },
        mip_level_count: u32::try_from(baked.mip_levels().len()).unwrap_or(1).max(1),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip, level) in baked.mip_levels().iter().enumerate() {
        let side = u32::from(level.side());
        let size = wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: layers,
        };
        if !level.texels().is_empty() {
            write_texture(queue, &indexed, mip as u32, side, size, level.texels());
        }
    }
    let row_count = u32::from(baked.palette_row_count()).max(1);
    let rows = texture_2d(
        device,
        "palette rows",
        (16, row_count),
        wgpu::TextureFormat::R16Uint,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let row_bytes: Vec<u8> = baked
        .palette_rows()
        .iter()
        .flat_map(|row| row.to_le_bytes())
        .collect();
    if !row_bytes.is_empty() {
        write_texture(
            queue,
            &rows,
            0,
            32,
            wgpu::Extent3d {
                width: 16,
                height: row_count,
                depth_or_array_layers: 1,
            },
            &row_bytes,
        );
    }
    let color_rows = u32::try_from(baked.palette_color_bytes().len() / 64)
        .unwrap_or(1)
        .max(1);
    let colors = texture_2d(
        device,
        "palette colors",
        (16, color_rows),
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    write_texture(
        queue,
        &colors,
        0,
        64,
        wgpu::Extent3d {
            width: 16,
            height: color_rows,
            depth_or_array_layers: 1,
        },
        baked.palette_color_bytes(),
    );
    let array_view = indexed.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..wgpu::TextureViewDescriptor::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("indexed textures"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&array_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &rows.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(
                    &colors.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
        ],
    })
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn zeroed_storage(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer_init(&BufferInitDescriptor {
        label: Some(label),
        contents: &vec![0; usize::try_from(size).unwrap_or(0)],
        usage: wgpu::BufferUsages::STORAGE,
    })
}

fn group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    resources: &[wgpu::BindingResource<'_>],
) -> wgpu::BindGroup {
    let entries: Vec<wgpu::BindGroupEntry<'_>> = resources
        .iter()
        .enumerate()
        .map(|(binding, resource)| wgpu::BindGroupEntry {
            binding: binding as u32,
            resource: resource.clone(),
        })
        .collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &entries,
    })
}

/// Little-endian packing helper for WGSL uniform structs built from vec4/mat4 members.
#[derive(Default)]
struct Uniform(Vec<u8>);

impl Uniform {
    fn matrix(mut self, matrix: Mat4) -> Self {
        for value in matrix.to_cols_array() {
            self.0.extend_from_slice(&value.to_le_bytes());
        }
        self
    }

    fn vec4(mut self, values: [f32; 4]) -> Self {
        for value in values {
            self.0.extend_from_slice(&value.to_le_bytes());
        }
        self
    }

    fn uvec4(mut self, values: [u32; 4]) -> Self {
        for value in values {
            self.0.extend_from_slice(&value.to_le_bytes());
        }
        self
    }
}

fn extend(vector: Vec3, w: f32) -> [f32; 4] {
    [vector.x, vector.y, vector.z, w]
}

/// Texel-snapped orthographic sun view around the camera, so shadows do not shimmer as it moves.
fn shadow_view_projection(camera: Vec3, to_sun: Vec3) -> Mat4 {
    let up = if to_sun.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let rotation = look_to_mat4(Vec3::ZERO, -to_sun, up);
    let texel = 2.0 * SHADOW_RADIUS / SHADOW_SIZE as f32;
    let center = rotation.transform_point3(camera);
    let snapped = Vec3::new(
        (center.x / texel).floor() * texel,
        (center.y / texel).floor() * texel,
        center.z,
    );
    let center = rotation.inverse().transform_point3(snapped);
    let view = look_to_mat4(center + to_sun * SHADOW_DEPTH * 0.5, -to_sun, up);
    let projection = orthographic(
        -SHADOW_RADIUS,
        SHADOW_RADIUS,
        -SHADOW_RADIUS,
        SHADOW_RADIUS,
        0.0,
        SHADOW_DEPTH,
    );
    projection * view
}

impl Renderer {
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        display_format: wgpu::TextureFormat,
        size: (u32, u32),
        scene: &Scene,
    ) -> Result<Self, String> {
        let layouts = Layouts::new(&device);
        let pipelines = Pipelines::new(&device, &scene.shaders, &layouts, display_format)?;
        let buffers = FrameBuffers {
            surface: uniform_buffer(&device, "surface frame", 256),
            shadow: uniform_buffer(&device, "shadow frame", 64),
            sky: uniform_buffer(&device, "sky frame", 176),
            post: uniform_buffer(&device, "post frame", 64),
        };
        let shadow_view = texture_2d(
            &device,
            "shadow map",
            (SHADOW_SIZE, SHADOW_SIZE),
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&wgpu::TextureViewDescriptor::default());
        let bloom = texture_2d(
            &device,
            "neutral bloom",
            (1, 1),
            HDR_FORMAT,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        write_texture(
            &queue,
            &bloom,
            0,
            8,
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            &[0; 8],
        );
        let bloom_view = bloom.create_view(&wgpu::TextureViewDescriptor::default());
        let clamp = wgpu::AddressMode::ClampToEdge;
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow comparison"),
            address_mode_u: clamp,
            address_mode_v: clamp,
            address_mode_w: clamp,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..wgpu::SamplerDescriptor::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            address_mode_u: clamp,
            address_mode_v: clamp,
            address_mode_w: clamp,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let point_lights = zeroed_storage(&device, "point lights", 32);
        // The surface shader returns before indexing tiles when light_grid.z is zero.
        let tile_counts = zeroed_storage(&device, "disabled tile light counts", 4);
        let tile_indices = zeroed_storage(&device, "disabled tile light indices", 4);
        let groups = Groups {
            surface_frame: group(
                &device,
                &layouts.surface_frame,
                &[
                    buffers.surface.as_entire_binding(),
                    wgpu::BindingResource::TextureView(&shadow_view),
                    wgpu::BindingResource::Sampler(&shadow_sampler),
                ],
            ),
            textures: indexed_texture_group(&device, &queue, &layouts.textures, &scene.textures),
            lights: group(
                &device,
                &layouts.lights,
                &[
                    point_lights.as_entire_binding(),
                    tile_counts.as_entire_binding(),
                    tile_indices.as_entire_binding(),
                ],
            ),
            shadow_frame: group(
                &device,
                &layouts.shadow_frame,
                &[buffers.shadow.as_entire_binding()],
            ),
            sky_frame: group(
                &device,
                &layouts.sky_frame,
                &[buffers.sky.as_entire_binding()],
            ),
            post_frame: group(
                &device,
                &layouts.post_frame,
                &[buffers.post.as_entire_binding()],
            ),
        };
        let targets = Self::create_targets(&device, &layouts, &bloom_view, &linear_sampler, size);
        let terrain = drawables(&device, &scene.terrain);
        let trees = drawables(&device, &scene.trees);
        Ok(Self {
            device,
            queue,
            layouts,
            pipelines,
            buffers,
            groups,
            shadow_view,
            bloom_view,
            linear_sampler,
            targets,
            terrain,
            trees,
        })
    }

    fn create_targets(
        device: &wgpu::Device,
        layouts: &Layouts,
        bloom_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        (width, height): (u32, u32),
    ) -> Targets {
        let (width, height) = (width.max(1), height.max(1));
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let hdr = texture_2d(device, "hdr color", (width, height), HDR_FORMAT, usage)
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = texture_2d(device, "scene depth", (width, height), DEPTH_FORMAT, usage)
            .create_view(&wgpu::TextureViewDescriptor::default());
        let post_textures = group(
            device,
            &layouts.post_textures,
            &[
                wgpu::BindingResource::TextureView(&hdr),
                wgpu::BindingResource::TextureView(bloom_view),
                wgpu::BindingResource::Sampler(sampler),
            ],
        );
        Targets {
            width,
            height,
            hdr,
            depth,
            post_textures,
        }
    }

    pub fn resize(&mut self, size: (u32, u32)) {
        if size != (self.targets.width, self.targets.height) && size.0 > 0 && size.1 > 0 {
            self.targets = Self::create_targets(
                &self.device,
                &self.layouts,
                &self.bloom_view,
                &self.linear_sampler,
                size,
            );
        }
    }

    fn write_uniforms(&self, input: &FrameInput<'_>, view_projection: Mat4, shadow: Mat4) {
        let daylight = input.daylight;
        let camera = input.camera.position;
        let (width, height) = (self.targets.width as f32, self.targets.height as f32);
        let light_direction = extend(-daylight.to_sun, daylight.sun_intensity);
        let tiles_x = self.targets.width.div_ceil(16);
        let surface = Uniform::default()
            .matrix(view_projection)
            .matrix(shadow)
            .vec4(extend(camera, input.time_seconds))
            .vec4(light_direction)
            .vec4(extend(daylight.sun_color, 1.0))
            .vec4(extend(daylight.sky_ambient, 1.0))
            // Thin haze that thickens in valleys; distance fog reaches the horizon color.
            .vec4(extend(daylight.horizon, 0.0028))
            .vec4([-12.0, 0.035, FAR, 0.0])
            .vec4([width, height, 1.0 / width, 1.0 / height])
            .uvec4([tiles_x, self.targets.height.div_ceil(16), 0, 0]);
        self.queue
            .write_buffer(&self.buffers.surface, 0, &surface.0);
        self.queue.write_buffer(
            &self.buffers.shadow,
            0,
            &Uniform::default().matrix(shadow).0,
        );
        let sky = Uniform::default()
            .matrix(view_projection.inverse())
            .vec4(extend(camera, input.time_seconds))
            .vec4(light_direction)
            .vec4(extend(daylight.sun_color, 1.0))
            .vec4(extend(daylight.zenith, 1.0))
            .vec4(extend(daylight.horizon, 1.0))
            .vec4([0.56, 0.006, 0.0025, 0.85])
            .vec4([0.008, 0.012, 0.030, daylight.night]);
        self.queue.write_buffer(&self.buffers.sky, 0, &sky.0);
        let post = Uniform::default()
            .vec4([daylight.exposure, 0.0, 1.0, 1.0])
            .vec4([daylight.saturation, 1.10, 1.0, 0.0])
            .vec4([0.28, 0.55, 1.0, 0.0])
            .uvec4([
                self.targets.width,
                self.targets.height,
                input.frame_index,
                0,
            ]);
        self.queue.write_buffer(&self.buffers.post, 0, &post.0);
    }

    /// Records and submits one frame into `display` (any texture of the display format).
    pub fn render(&self, display: &wgpu::TextureView, input: &FrameInput<'_>) {
        let aspect = self.targets.width as f32 / self.targets.height as f32;
        let view_projection = input.camera.view_projection(aspect);
        let shadow = shadow_view_projection(input.camera.position, input.daylight.to_sun);
        self.write_uniforms(input, view_projection, shadow);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        if input.daylight.sun_intensity > 0.001 {
            self.shadow_pass(&mut encoder, &Frustum::new(shadow));
        } else {
            self.clear_shadow(&mut encoder);
        }
        self.sky_pass(&mut encoder);
        self.surface_pass(&mut encoder, &Frustum::new(view_projection));
        self.post_pass(&mut encoder, display);
        self.queue.submit(Some(encoder.finish()));
    }

    fn visible<'a>(&'a self, frustum: &'a Frustum) -> impl Iterator<Item = &'a Drawable> {
        self.terrain
            .iter()
            .chain(&self.trees)
            .filter(|drawable| frustum.intersects(drawable.min, drawable.max))
    }

    fn depth_only_pass<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.shadow_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Night keeps the comparison texture valid at depth 1.0 (fully lit), as the contract requires.
    fn clear_shadow(&self, encoder: &mut wgpu::CommandEncoder) {
        drop(self.depth_only_pass(encoder));
    }

    fn shadow_pass(&self, encoder: &mut wgpu::CommandEncoder, frustum: &Frustum) {
        let mut pass = self.depth_only_pass(encoder);
        pass.set_pipeline(&self.pipelines.shadow_opaque);
        pass.set_bind_group(0, &self.groups.shadow_frame, &[]);
        for drawable in self.visible(frustum) {
            draw(&mut pass, drawable.opaque.as_ref());
        }
        pass.set_pipeline(&self.pipelines.shadow_cutout);
        pass.set_bind_group(0, &self.groups.shadow_frame, &[]);
        pass.set_bind_group(1, &self.groups.textures, &[]);
        for drawable in self.visible(frustum) {
            draw(&mut pass, drawable.cutout.as_ref());
        }
    }

    fn sky_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky"),
            color_attachments: &[Some(color_attachment(
                &self.targets.hdr,
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            ))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipelines.sky);
        pass.set_bind_group(0, &self.groups.sky_frame, &[]);
        pass.draw(0..3, 0..1);
    }

    fn surface_pass(&self, encoder: &mut wgpu::CommandEncoder, frustum: &Frustum) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("surfaces"),
            color_attachments: &[Some(color_attachment(
                &self.targets.hdr,
                wgpu::LoadOp::Load,
            ))],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.groups.surface_frame, &[]);
        pass.set_bind_group(1, &self.groups.textures, &[]);
        pass.set_bind_group(2, &self.groups.lights, &[]);
        pass.set_pipeline(&self.pipelines.surface_opaque);
        for drawable in self.visible(frustum) {
            draw(&mut pass, drawable.opaque.as_ref());
        }
        pass.set_pipeline(&self.pipelines.surface_cutout);
        for drawable in self.visible(frustum) {
            draw(&mut pass, drawable.cutout.as_ref());
        }
    }

    fn post_pass(&self, encoder: &mut wgpu::CommandEncoder, display: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("post process"),
            color_attachments: &[Some(color_attachment(
                display,
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            ))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipelines.post);
        pass.set_bind_group(0, &self.groups.post_frame, &[]);
        pass.set_bind_group(1, &self.targets.post_textures, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn color_attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    }
}

fn draw(pass: &mut wgpu::RenderPass<'_>, mesh: Option<&GpuMesh>) {
    if let Some(mesh) = mesh {
        pass.set_vertex_buffer(0, mesh.vertex.slice(..));
        pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.count, 0, 0..1);
    }
}
