//! Defines built-in bounded WGSL libraries and executable shader programs.

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
    // Texture geometry is injected into the shared shader prelude. Changing TEXTURE_SIDE or the
    // mip chain requires rerunning the dedicated built-in shader validation target.
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

#[cfg(test)]
#[path = "shaders_tests.rs"]
mod tests;
