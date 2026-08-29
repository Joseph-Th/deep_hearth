//! Contract tests for built-in shader content.

use super::*;
use crate::shader::ShaderProgramKind;

#[cfg(feature = "test-shader-validation")]
#[test]
fn built_in_programs_assemble_and_validate_as_portable_wgsl() {
    assert_eq!(
        validate_builtin_shader_programs(),
        Ok(EXECUTABLE_PROGRAMS.len())
    );
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
