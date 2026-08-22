//! Built-in bounded WGSL libraries and executable programs for the sibling shader registry.

#[cfg(feature = "test-shader-validation")]
use std::error::Error;
#[cfg(feature = "test-shader-validation")]
use std::fmt::{Display, Formatter};

#[cfg(feature = "test-shader-validation")]
use naga::ShaderStage;
#[cfg(feature = "test-shader-validation")]
use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::shader::{
    ComputeEntryPoint, RenderEntryPoints, RenderPipelineProfile, ShaderBlendMode,
    ShaderColorTarget, ShaderDefinition, ShaderDepthMode, ShaderId, ShaderRegistry,
    ShaderWorkBudget,
};
use crate::texture::{TEXTURE_MIP_LEVEL_COUNT, TEXTURE_SIDE};

const SHADER_COMMON: ShaderId = ShaderId::new(1);
const SHADER_NOISE: ShaderId = ShaderId::new(2);
const SHADER_LIGHTING: ShaderId = ShaderId::new(3);
const SHADER_INDEXED_TEXTURE: ShaderId = ShaderId::new(4);

pub const SHADER_SURFACE: ShaderId = ShaderId::new(100);
pub const SHADER_LIGHT_CULL: ShaderId = ShaderId::new(101);
pub const SHADER_WATER: ShaderId = ShaderId::new(102);
pub const SHADER_SMOKE: ShaderId = ShaderId::new(103);
pub const SHADER_SKY: ShaderId = ShaderId::new(104);
pub const SHADER_POST_PROCESS: ShaderId = ShaderId::new(105);
pub const SHADER_BLOOM: ShaderId = ShaderId::new(106);
pub const SHADER_SHADOW: ShaderId = ShaderId::new(107);
pub const SHADER_SHADOW_CUTOUT: ShaderId = ShaderId::new(108);

#[cfg(feature = "test-shader-validation")]
const EXECUTABLE_PROGRAMS: [ShaderId; 9] = [
    SHADER_SURFACE,
    SHADER_LIGHT_CULL,
    SHADER_WATER,
    SHADER_SMOKE,
    SHADER_SKY,
    SHADER_POST_PROCESS,
    SHADER_BLOOM,
    SHADER_SHADOW,
    SHADER_SHADOW_CUTOUT,
];

const COMMON_SOURCE: &str = include_str!("../../assets/shaders/common.wgsl");
const NOISE_SOURCE: &str = include_str!("../../assets/shaders/noise.wgsl");
const LIGHTING_SOURCE: &str = include_str!("../../assets/shaders/lighting.wgsl");
const INDEXED_TEXTURE_SOURCE: &str = include_str!("../../assets/shaders/indexed_texture.wgsl");
const SURFACE_SOURCE: &str = include_str!("../../assets/shaders/surface.wgsl");
const LIGHT_CULL_SOURCE: &str = include_str!("../../assets/shaders/light_cull.wgsl");
const WATER_SOURCE: &str = include_str!("../../assets/shaders/water.wgsl");
const SMOKE_SOURCE: &str = include_str!("../../assets/shaders/smoke.wgsl");
const SKY_SOURCE: &str = include_str!("../../assets/shaders/sky.wgsl");
const POST_PROCESS_SOURCE: &str = include_str!("../../assets/shaders/post_process.wgsl");
const BLOOM_SOURCE: &str = include_str!("../../assets/shaders/bloom.wgsl");
const SHADOW_CUTOUT_SOURCE: &str = include_str!("../../assets/shaders/shadow_cutout.wgsl");
const SHADOW_OPAQUE_SOURCE: &str = include_str!("../../assets/shaders/shadow_opaque.wgsl");

fn build_common_source() -> String {
    format!(
        "const DH_INDEXED_TEXTURE_SIDE: u32 = {TEXTURE_SIDE}u;\n\
         const DH_INDEXED_TEXTURE_MAX_MIP: u32 = {}u;\n\n{COMMON_SOURCE}",
        TEXTURE_MIP_LEVEL_COUNT - 1,
    )
}

pub(crate) fn build_shader_registry() -> ShaderRegistry {
    let common_source = build_common_source();
    ShaderRegistry::new([
        ShaderDefinition::new_library(SHADER_COMMON, "common", Vec::new(), common_source),
        ShaderDefinition::new_library(SHADER_NOISE, "noise", vec![SHADER_COMMON], NOISE_SOURCE),
        ShaderDefinition::new_library(
            SHADER_LIGHTING,
            "lighting",
            vec![SHADER_COMMON],
            LIGHTING_SOURCE,
        ),
        ShaderDefinition::new_library(
            SHADER_INDEXED_TEXTURE,
            "indexed texture sampling",
            vec![SHADER_COMMON],
            INDEXED_TEXTURE_SOURCE,
        ),
        ShaderDefinition::new_render(
            SHADER_SURFACE,
            "indexed surface",
            vec![SHADER_COMMON, SHADER_LIGHTING, SHADER_INDEXED_TEXTURE],
            SURFACE_SOURCE,
            RenderEntryPoints::new("surface_vs", "surface_fs"),
            RenderPipelineProfile::new(
                ShaderBlendMode::Opaque,
                ShaderDepthMode::ReadWrite,
                ShaderColorTarget::LinearHdr,
            ),
            ShaderWorkBudget::new(7, 0, 32, 32),
        ),
        ShaderDefinition::new_compute(
            SHADER_LIGHT_CULL,
            "tiled light cull",
            vec![SHADER_COMMON, SHADER_LIGHTING],
            LIGHT_CULL_SOURCE,
            ComputeEntryPoint::new("light_cull_cs", [64, 1, 1]),
            ShaderWorkBudget::new(0, 0, 0, 8),
        ),
        ShaderDefinition::new_render(
            SHADER_WATER,
            "analytic water",
            vec![SHADER_COMMON, SHADER_NOISE, SHADER_LIGHTING],
            WATER_SOURCE,
            RenderEntryPoints::new("water_vs", "water_fs"),
            RenderPipelineProfile::new(
                ShaderBlendMode::PremultipliedAlpha,
                ShaderDepthMode::ReadOnly,
                ShaderColorTarget::LinearHdr,
            ),
            ShaderWorkBudget::new(2, 1, 0, 0),
        ),
        ShaderDefinition::new_render(
            SHADER_SMOKE,
            "procedural smoke",
            vec![SHADER_COMMON, SHADER_NOISE],
            SMOKE_SOURCE,
            RenderEntryPoints::new("smoke_vs", "smoke_fs"),
            RenderPipelineProfile::new(
                ShaderBlendMode::PremultipliedAlpha,
                ShaderDepthMode::ReadOnly,
                ShaderColorTarget::LinearHdr,
            ),
            ShaderWorkBudget::new(1, 3, 0, 0),
        ),
        ShaderDefinition::new_render(
            SHADER_SKY,
            "procedural sky",
            vec![SHADER_COMMON, SHADER_NOISE],
            SKY_SOURCE,
            RenderEntryPoints::new("sky_vs", "sky_fs"),
            RenderPipelineProfile::new(
                ShaderBlendMode::Opaque,
                ShaderDepthMode::Disabled,
                ShaderColorTarget::LinearHdr,
            ),
            ShaderWorkBudget::new(0, 3, 0, 0),
        ),
        ShaderDefinition::new_render(
            SHADER_POST_PROCESS,
            "hdr post process",
            vec![SHADER_COMMON],
            POST_PROCESS_SOURCE,
            RenderEntryPoints::new("post_process_vs", "post_process_fs"),
            RenderPipelineProfile::new(
                ShaderBlendMode::Opaque,
                ShaderDepthMode::Disabled,
                ShaderColorTarget::Display,
            ),
            ShaderWorkBudget::new(5, 0, 0, 0),
        ),
        ShaderDefinition::new_compute(
            SHADER_BLOOM,
            "half-resolution bloom",
            vec![SHADER_COMMON],
            BLOOM_SOURCE,
            ComputeEntryPoint::new("bloom_downsample_cs", [8, 8, 1]),
            ShaderWorkBudget::new(4, 0, 0, 0),
        ),
        ShaderDefinition::new_render(
            SHADER_SHADOW,
            "opaque depth shadow",
            Vec::new(),
            SHADOW_OPAQUE_SOURCE,
            RenderEntryPoints::new_vertex_only("shadow_opaque_vs"),
            RenderPipelineProfile::new(
                ShaderBlendMode::Opaque,
                ShaderDepthMode::ReadWrite,
                ShaderColorTarget::None,
            ),
            ShaderWorkBudget::new(0, 0, 0, 0),
        ),
        ShaderDefinition::new_render(
            SHADER_SHADOW_CUTOUT,
            "indexed cutout shadow",
            vec![SHADER_COMMON, SHADER_INDEXED_TEXTURE],
            SHADOW_CUTOUT_SOURCE,
            RenderEntryPoints::new("shadow_cutout_vs", "shadow_cutout_fs"),
            RenderPipelineProfile::new(
                ShaderBlendMode::Opaque,
                ShaderDepthMode::ReadWrite,
                ShaderColorTarget::None,
            ),
            ShaderWorkBudget::new(3, 0, 0, 0),
        ),
    ])
}

/// Failure while validating the assembled built-in executable shader suite with Naga.
#[cfg(feature = "test-shader-validation")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuiltInShaderValidationError {
    ProgramCount {
        expected: usize,
        actual: usize,
    },
    MissingProgram {
        shader: ShaderId,
    },
    Parse {
        shader: ShaderId,
        message: String,
    },
    Validation {
        shader: ShaderId,
        message: String,
    },
    ExecutableLibrary {
        shader: ShaderId,
    },
    MissingEntryPoint {
        shader: ShaderId,
        stage: &'static str,
        entry_point: String,
    },
    UnexpectedFragmentEntryPoint {
        shader: ShaderId,
    },
}

#[cfg(feature = "test-shader-validation")]
impl Display for BuiltInShaderValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProgramCount { expected, actual } => write!(
                formatter,
                "built-in shader bake produced {actual} executable programs; expected {expected}"
            ),
            Self::MissingProgram { shader } => {
                write!(
                    formatter,
                    "built-in executable shader {} was not baked",
                    shader.value()
                )
            }
            Self::Parse { shader, message } => write!(
                formatter,
                "shader {} failed WGSL parsing: {message}",
                shader.value()
            ),
            Self::Validation { shader, message } => write!(
                formatter,
                "shader {} failed WGSL validation: {message}",
                shader.value()
            ),
            Self::ExecutableLibrary { shader } => write!(
                formatter,
                "executable shader {} baked as a library",
                shader.value()
            ),
            Self::MissingEntryPoint {
                shader,
                stage,
                entry_point,
            } => write!(
                formatter,
                "shader {} is missing {stage} entry point {entry_point}",
                shader.value()
            ),
            Self::UnexpectedFragmentEntryPoint { shader } => write!(
                formatter,
                "vertex-only shader {} unexpectedly contains a fragment entry point",
                shader.value()
            ),
        }
    }
}

#[cfg(feature = "test-shader-validation")]
impl Error for BuiltInShaderValidationError {}

#[cfg(feature = "test-shader-validation")]
fn require_entry_point(
    shader: ShaderId,
    module: &naga::Module,
    stage: ShaderStage,
    stage_name: &'static str,
    entry_point: &str,
) -> Result<(), BuiltInShaderValidationError> {
    if module
        .entry_points
        .iter()
        .any(|entry| entry.stage == stage && entry.name == entry_point)
    {
        return Ok(());
    }
    Err(BuiltInShaderValidationError::MissingEntryPoint {
        shader,
        stage: stage_name,
        entry_point: entry_point.to_owned(),
    })
}

/// Parses and semantically validates every assembled built-in executable shader with portable Naga
/// capabilities. This lives outside `#[cfg(test)]` so the dedicated validation target does not need
/// to compile the crate's unrelated unit-test harness.
#[cfg(feature = "test-shader-validation")]
pub fn validate_builtin_shader_programs() -> Result<usize, BuiltInShaderValidationError> {
    let baked = build_shader_registry().bake_shader_set();
    let actual = baked.program_count();
    if actual != EXECUTABLE_PROGRAMS.len() {
        return Err(BuiltInShaderValidationError::ProgramCount {
            expected: EXECUTABLE_PROGRAMS.len(),
            actual,
        });
    }

    for shader in EXECUTABLE_PROGRAMS {
        let program = baked
            .get_program(shader)
            .ok_or(BuiltInShaderValidationError::MissingProgram { shader })?;
        let module = naga::front::wgsl::parse_str(program.source()).map_err(|error| {
            BuiltInShaderValidationError::Parse {
                shader,
                message: error.emit_to_string(program.source()),
            }
        })?;
        Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .map_err(|error| BuiltInShaderValidationError::Validation {
                shader,
                message: format!("{error:?}"),
            })?;

        match program.kind() {
            crate::shader::ShaderProgramKind::Library => {
                return Err(BuiltInShaderValidationError::ExecutableLibrary { shader });
            }
            crate::shader::ShaderProgramKind::Render {
                entry_points,
                pipeline: _,
                work_budget: _,
            } => {
                require_entry_point(
                    shader,
                    &module,
                    ShaderStage::Vertex,
                    "vertex",
                    entry_points.vertex(),
                )?;
                if let Some(fragment) = entry_points.fragment() {
                    require_entry_point(
                        shader,
                        &module,
                        ShaderStage::Fragment,
                        "fragment",
                        fragment,
                    )?;
                } else if module
                    .entry_points
                    .iter()
                    .any(|entry| entry.stage == ShaderStage::Fragment)
                {
                    return Err(BuiltInShaderValidationError::UnexpectedFragmentEntryPoint {
                        shader,
                    });
                }
            }
            crate::shader::ShaderProgramKind::Compute {
                entry_point,
                work_budget: _,
            } => require_entry_point(
                shader,
                &module,
                ShaderStage::Compute,
                "compute",
                entry_point.name(),
            )?,
        }
    }
    Ok(EXECUTABLE_PROGRAMS.len())
}

#[cfg(test)]
#[path = "shaders_tests.rs"]
mod tests;
