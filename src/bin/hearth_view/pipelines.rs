//! Bind-group layouts and render pipelines built from the baked shader set.

use deep_hearth::content::{
    SHADER_POST_PROCESS, SHADER_SHADOW, SHADER_SHADOW_CUTOUT, SHADER_SKY, SHADER_SURFACE,
};
use deep_hearth::meshing::{SURFACE_VERTEX_OFFSETS, SURFACE_VERTEX_STRIDE};
use deep_hearth::shader::{BakedShaderSet, ShaderId, ShaderProgramKind};

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct Layouts {
    pub surface_frame: wgpu::BindGroupLayout,
    pub textures: wgpu::BindGroupLayout,
    pub lights: wgpu::BindGroupLayout,
    pub shadow_frame: wgpu::BindGroupLayout,
    pub sky_frame: wgpu::BindGroupLayout,
    pub post_frame: wgpu::BindGroupLayout,
    pub post_textures: wgpu::BindGroupLayout,
}

pub struct Pipelines {
    pub surface_opaque: wgpu::RenderPipeline,
    pub surface_cutout: wgpu::RenderPipeline,
    pub shadow_opaque: wgpu::RenderPipeline,
    pub shadow_cutout: wgpu::RenderPipeline,
    pub sky: wgpu::RenderPipeline,
    pub post: wgpu::RenderPipeline,
}

fn entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    }
}

fn uniform() -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn storage() -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn texture(
    sample_type: wgpu::TextureSampleType,
    dimension: wgpu::TextureViewDimension,
) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type,
        view_dimension: dimension,
        multisampled: false,
    }
}

fn layout(
    device: &wgpu::Device,
    label: &str,
    entries: &[wgpu::BindGroupLayoutEntry],
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries,
    })
}

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        use wgpu::ShaderStages as Stage;
        use wgpu::TextureSampleType as Sample;
        use wgpu::TextureViewDimension as Dim;
        let both = Stage::VERTEX | Stage::FRAGMENT;
        Self {
            surface_frame: layout(
                device,
                "surface frame",
                &[
                    entry(0, both, uniform()),
                    entry(1, Stage::FRAGMENT, texture(Sample::Depth, Dim::D2)),
                    entry(
                        2,
                        Stage::FRAGMENT,
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    ),
                ],
            ),
            textures: layout(
                device,
                "indexed textures",
                &[
                    entry(0, Stage::FRAGMENT, texture(Sample::Uint, Dim::D2Array)),
                    entry(1, Stage::FRAGMENT, texture(Sample::Uint, Dim::D2)),
                    entry(
                        2,
                        Stage::FRAGMENT,
                        texture(Sample::Float { filterable: false }, Dim::D2),
                    ),
                ],
            ),
            lights: layout(
                device,
                "tiled lights",
                &[
                    entry(0, Stage::FRAGMENT, storage()),
                    entry(1, Stage::FRAGMENT, storage()),
                    entry(2, Stage::FRAGMENT, storage()),
                ],
            ),
            shadow_frame: layout(
                device,
                "shadow frame",
                &[entry(0, Stage::VERTEX, uniform())],
            ),
            sky_frame: layout(device, "sky frame", &[entry(0, both, uniform())]),
            post_frame: layout(
                device,
                "post frame",
                &[entry(0, Stage::FRAGMENT, uniform())],
            ),
            post_textures: layout(
                device,
                "post textures",
                &[
                    entry(
                        0,
                        Stage::FRAGMENT,
                        texture(Sample::Float { filterable: true }, Dim::D2),
                    ),
                    entry(
                        1,
                        Stage::FRAGMENT,
                        texture(Sample::Float { filterable: true }, Dim::D2),
                    ),
                    entry(
                        2,
                        Stage::FRAGMENT,
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    ),
                ],
            ),
        }
    }
}

struct Program {
    module: wgpu::ShaderModule,
    vertex: String,
    fragment: Option<String>,
}

fn program(
    device: &wgpu::Device,
    shaders: &BakedShaderSet,
    id: ShaderId,
) -> Result<Program, String> {
    let baked = shaders
        .get_program(id)
        .ok_or_else(|| format!("shader {} is not baked", id.value()))?;
    let ShaderProgramKind::Render { entry_points, .. } = baked.kind() else {
        return Err(format!("shader {} is not a render program", baked.name()));
    };
    Ok(Program {
        module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(baked.name()),
            source: wgpu::ShaderSource::Wgsl(baked.source().into()),
        }),
        vertex: entry_points.vertex().to_owned(),
        fragment: entry_points.fragment().map(str::to_owned),
    })
}

const SURFACE_FORMATS: [wgpu::VertexFormat; 5] = [
    wgpu::VertexFormat::Float32x3,
    wgpu::VertexFormat::Float32x4,
    wgpu::VertexFormat::Float32x4,
    wgpu::VertexFormat::Uint32,
    wgpu::VertexFormat::Float32x4,
];

fn attributes(locations: &[u32]) -> Vec<wgpu::VertexAttribute> {
    locations
        .iter()
        .map(|&location| wgpu::VertexAttribute {
            format: SURFACE_FORMATS[location as usize],
            offset: SURFACE_VERTEX_OFFSETS[location as usize] as u64,
            shader_location: location,
        })
        .collect()
}

fn depth(write: bool, bias: wgpu::DepthBiasState) -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: wgpu::StencilState::default(),
        bias,
    })
}

struct PipelineSpec<'a> {
    label: &'a str,
    program: &'a Program,
    layout: &'a wgpu::PipelineLayout,
    vertex_locations: &'a [u32],
    cull: Option<wgpu::Face>,
    depth: Option<wgpu::DepthStencilState>,
    target: Option<wgpu::TextureFormat>,
}

fn pipeline(device: &wgpu::Device, spec: PipelineSpec<'_>) -> wgpu::RenderPipeline {
    let attributes = attributes(spec.vertex_locations);
    let buffers = if attributes.is_empty() {
        Vec::new()
    } else {
        vec![Some(wgpu::VertexBufferLayout {
            array_stride: SURFACE_VERTEX_STRIDE as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        })]
    };
    let targets = spec.target.map(|format| {
        Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })
    });
    let target_slice: &[Option<wgpu::ColorTargetState>] = match &targets {
        Some(target) => std::slice::from_ref(target),
        None => &[],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(spec.label),
        layout: Some(spec.layout),
        vertex: wgpu::VertexState {
            module: &spec.program.module,
            entry_point: Some(&spec.program.vertex),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: spec.cull,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: spec.depth,
        multisample: wgpu::MultisampleState::default(),
        fragment: spec
            .program
            .fragment
            .as_deref()
            .map(|entry_point| wgpu::FragmentState {
                module: &spec.program.module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: target_slice,
            }),
        multiview_mask: None,
        cache: None,
    })
}

fn pipeline_layout(
    device: &wgpu::Device,
    groups: &[&wgpu::BindGroupLayout],
) -> wgpu::PipelineLayout {
    let groups: Vec<Option<&wgpu::BindGroupLayout>> = groups.iter().copied().map(Some).collect();
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &groups,
        immediate_size: 0,
    })
}

impl Pipelines {
    pub fn new(
        device: &wgpu::Device,
        shaders: &BakedShaderSet,
        layouts: &Layouts,
        display_format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let surface = program(device, shaders, SHADER_SURFACE)?;
        let shadow = program(device, shaders, SHADER_SHADOW)?;
        let shadow_cutout = program(device, shaders, SHADER_SHADOW_CUTOUT)?;
        let sky = program(device, shaders, SHADER_SKY)?;
        let post = program(device, shaders, SHADER_POST_PROCESS)?;
        let surface_layout = pipeline_layout(
            device,
            &[&layouts.surface_frame, &layouts.textures, &layouts.lights],
        );
        let shadow_layout = pipeline_layout(device, &[&layouts.shadow_frame]);
        let cutout_shadow_layout =
            pipeline_layout(device, &[&layouts.shadow_frame, &layouts.textures]);
        let sky_layout = pipeline_layout(device, &[&layouts.sky_frame]);
        let post_layout = pipeline_layout(device, &[&layouts.post_frame, &layouts.post_textures]);
        let all = [0, 1, 2, 3, 4];
        let shadow_bias = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 2.0,
            clamp: 0.0,
        };
        let surface_spec = |label, cull| PipelineSpec {
            label,
            program: &surface,
            layout: &surface_layout,
            vertex_locations: &all,
            cull,
            depth: depth(true, wgpu::DepthBiasState::default()),
            target: Some(HDR_FORMAT),
        };
        Ok(Self {
            surface_opaque: pipeline(
                device,
                surface_spec("surface opaque", Some(wgpu::Face::Back)),
            ),
            surface_cutout: pipeline(device, surface_spec("surface cutout", None)),
            shadow_opaque: pipeline(
                device,
                PipelineSpec {
                    label: "shadow opaque",
                    program: &shadow,
                    layout: &shadow_layout,
                    vertex_locations: &[0],
                    cull: None,
                    depth: depth(true, shadow_bias),
                    target: None,
                },
            ),
            shadow_cutout: pipeline(
                device,
                PipelineSpec {
                    label: "shadow cutout",
                    program: &shadow_cutout,
                    layout: &cutout_shadow_layout,
                    vertex_locations: &[0, 1, 3, 4],
                    cull: None,
                    depth: depth(true, shadow_bias),
                    target: None,
                },
            ),
            sky: pipeline(
                device,
                PipelineSpec {
                    label: "sky",
                    program: &sky,
                    layout: &sky_layout,
                    vertex_locations: &[],
                    cull: None,
                    depth: None,
                    target: Some(HDR_FORMAT),
                },
            ),
            post: pipeline(
                device,
                PipelineSpec {
                    label: "post process",
                    program: &post,
                    layout: &post_layout,
                    vertex_locations: &[],
                    cull: None,
                    depth: None,
                    target: Some(display_format),
                },
            ),
        })
    }
}
