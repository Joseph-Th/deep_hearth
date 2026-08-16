//! Built-in bounded WGSL libraries and executable programs for the sibling shader registry.

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
mod tests {
    #[cfg(feature = "test-shader-validation")]
    use naga::ShaderStage;
    #[cfg(feature = "test-shader-validation")]
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    use super::*;
    use crate::shader::ShaderProgramKind;

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

    #[cfg(feature = "test-shader-validation")]
    #[test]
    fn built_in_programs_assemble_and_validate_as_portable_wgsl() {
        let registry = build_shader_registry();
        let baked = registry.bake_shader_set();

        assert_eq!(baked.program_count(), EXECUTABLE_PROGRAMS.len());
        for id in EXECUTABLE_PROGRAMS {
            let program = match baked.get_program(id) {
                Some(program) => program,
                None => panic!("built-in executable shader {} was not baked", id.value()),
            };
            let module = match naga::front::wgsl::parse_str(program.source()) {
                Ok(module) => module,
                Err(error) => panic!(
                    "shader {} failed WGSL parsing:\n{}",
                    id.value(),
                    error.emit_to_string(program.source())
                ),
            };
            if let Err(error) =
                Validator::new(ValidationFlags::all(), Capabilities::empty()).validate(&module)
            {
                panic!("shader {} failed WGSL validation: {error:?}", id.value());
            }

            match program.kind() {
                ShaderProgramKind::Library => {
                    panic!("executable shader {} baked as a library", id.value())
                }
                ShaderProgramKind::Render {
                    entry_points,
                    pipeline: _,
                    work_budget: _,
                } => {
                    assert!(module.entry_points.iter().any(|entry| {
                        entry.stage == ShaderStage::Vertex && entry.name == entry_points.vertex()
                    }));
                    match entry_points.fragment() {
                        Some(fragment) => assert!(module.entry_points.iter().any(|entry| {
                            entry.stage == ShaderStage::Fragment && entry.name == fragment
                        })),
                        None => assert!(
                            !module
                                .entry_points
                                .iter()
                                .any(|entry| entry.stage == ShaderStage::Fragment)
                        ),
                    }
                }
                ShaderProgramKind::Compute {
                    entry_point,
                    work_budget: _,
                } => {
                    assert!(module.entry_points.iter().any(|entry| {
                        entry.stage == ShaderStage::Compute && entry.name == entry_point.name()
                    }));
                }
            }
        }
    }

    #[test]
    fn built_in_work_budgets_capture_the_hot_path_limits() {
        let registry = build_shader_registry();

        for (id, expected) in [
            (SHADER_SURFACE, ShaderWorkBudget::new(7, 0, 32, 32)),
            (SHADER_LIGHT_CULL, ShaderWorkBudget::new(0, 0, 0, 8)),
            (SHADER_WATER, ShaderWorkBudget::new(2, 1, 0, 0)),
            (SHADER_SMOKE, ShaderWorkBudget::new(1, 3, 0, 0)),
            (SHADER_SKY, ShaderWorkBudget::new(0, 3, 0, 0)),
            (SHADER_POST_PROCESS, ShaderWorkBudget::new(5, 0, 0, 0)),
            (SHADER_BLOOM, ShaderWorkBudget::new(4, 0, 0, 0)),
            (SHADER_SHADOW, ShaderWorkBudget::new(0, 0, 0, 0)),
            (SHADER_SHADOW_CUTOUT, ShaderWorkBudget::new(3, 0, 0, 0)),
        ] {
            let definition = match registry.get_shader(id) {
                Some(definition) => definition,
                None => panic!("built-in shader definition {} did not resolve", id.value()),
            };
            assert_eq!(definition.kind().work_budget(), Some(expected));
        }
    }

    #[test]
    fn built_in_render_profiles_define_each_pass_attachment_contract() {
        let registry = build_shader_registry();

        for (id, expected) in [
            (
                SHADER_SURFACE,
                RenderPipelineProfile::new(
                    ShaderBlendMode::Opaque,
                    ShaderDepthMode::ReadWrite,
                    ShaderColorTarget::LinearHdr,
                ),
            ),
            (
                SHADER_WATER,
                RenderPipelineProfile::new(
                    ShaderBlendMode::PremultipliedAlpha,
                    ShaderDepthMode::ReadOnly,
                    ShaderColorTarget::LinearHdr,
                ),
            ),
            (
                SHADER_SMOKE,
                RenderPipelineProfile::new(
                    ShaderBlendMode::PremultipliedAlpha,
                    ShaderDepthMode::ReadOnly,
                    ShaderColorTarget::LinearHdr,
                ),
            ),
            (
                SHADER_SKY,
                RenderPipelineProfile::new(
                    ShaderBlendMode::Opaque,
                    ShaderDepthMode::Disabled,
                    ShaderColorTarget::LinearHdr,
                ),
            ),
            (
                SHADER_POST_PROCESS,
                RenderPipelineProfile::new(
                    ShaderBlendMode::Opaque,
                    ShaderDepthMode::Disabled,
                    ShaderColorTarget::Display,
                ),
            ),
            (
                SHADER_SHADOW,
                RenderPipelineProfile::new(
                    ShaderBlendMode::Opaque,
                    ShaderDepthMode::ReadWrite,
                    ShaderColorTarget::None,
                ),
            ),
            (
                SHADER_SHADOW_CUTOUT,
                RenderPipelineProfile::new(
                    ShaderBlendMode::Opaque,
                    ShaderDepthMode::ReadWrite,
                    ShaderColorTarget::None,
                ),
            ),
        ] {
            let definition = match registry.get_shader(id) {
                Some(definition) => definition,
                None => panic!("built-in shader definition {} did not resolve", id.value()),
            };
            match definition.kind() {
                ShaderProgramKind::Library => {
                    panic!("render shader {} resolved as a library", id.value())
                }
                ShaderProgramKind::Render {
                    entry_points: _,
                    pipeline,
                    work_budget: _,
                } => assert_eq!(*pipeline, expected),
                ShaderProgramKind::Compute {
                    entry_point: _,
                    work_budget: _,
                } => panic!("render shader {} resolved as compute", id.value()),
            }
        }
    }

    #[test]
    fn built_in_source_suite_stays_under_the_lightweight_shipping_budget() {
        let unique_source_bytes = [
            NOISE_SOURCE,
            LIGHTING_SOURCE,
            INDEXED_TEXTURE_SOURCE,
            SURFACE_SOURCE,
            LIGHT_CULL_SOURCE,
            WATER_SOURCE,
            SMOKE_SOURCE,
            SKY_SOURCE,
            POST_PROCESS_SOURCE,
            BLOOM_SOURCE,
            SHADOW_CUTOUT_SOURCE,
            SHADOW_OPAQUE_SOURCE,
        ]
        .iter()
        .map(|source| source.len())
        .sum::<usize>()
            + build_common_source().len();

        assert!(unique_source_bytes <= 48 * 1_024);
    }

    #[test]
    fn shader_texture_dimensions_are_derived_from_the_texture_upload_contract() {
        let common = build_common_source();

        assert!(common.contains(&format!(
            "const DH_INDEXED_TEXTURE_SIDE: u32 = {TEXTURE_SIDE}u;"
        )));
        assert!(common.contains(&format!(
            "const DH_INDEXED_TEXTURE_MAX_MIP: u32 = {}u;",
            TEXTURE_MIP_LEVEL_COUNT - 1
        )));
    }

    #[test]
    fn cutout_shadow_reuses_surface_mesh_texture_inputs() {
        for source in [SURFACE_SOURCE, SHADOW_CUTOUT_SOURCE] {
            assert!(source.contains("@location(1) uv_light: vec4<f32>"));
            assert!(source.contains("@location(3) texture_key: u32"));
        }
        assert!(SHADOW_OPAQUE_SOURCE.contains("@location(0) world_position: vec3<f32>"));
        assert!(!SHADOW_OPAQUE_SOURCE.contains("@fragment"));
    }
}
