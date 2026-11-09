//! Contract tests for built-in shader content.

use super::*;
use crate::shader::ShaderProgramKind;

#[test]
fn built_in_work_budgets_capture_the_hot_path_limits() {
    let registry = build_shader_registry();

    for (id, expected) in [
        (SHADER_SURFACE, ShaderWorkBudget::new(8, 0, 32, 32)),
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
        assert!(source.contains("@location(4) tint: vec4<f32>"));
    }
    assert!(SHADOW_OPAQUE_SOURCE.contains("@location(0) world_position: vec3<f32>"));
    assert!(!SHADOW_OPAQUE_SOURCE.contains("@fragment"));
}

#[test]
fn tinted_cutout_shadow_uses_the_same_alpha_discard_rule_as_surface() {
    assert!(
        INDEXED_TEXTURE_SOURCE.contains("const DH_INDEXED_ALPHA_DISCARD_THRESHOLD: f32 = 0.01;")
    );
    assert!(SURFACE_SOURCE.contains("base.a < DH_INDEXED_ALPHA_DISCARD_THRESHOLD"));
    assert!(SHADOW_CUTOUT_SOURCE.contains("output.tint_alpha = input.tint.a;"));
    assert!(
        SHADOW_CUTOUT_SOURCE
            .contains("alpha * input.tint_alpha < DH_INDEXED_ALPHA_DISCARD_THRESHOLD")
    );
}

#[test]
fn surface_light_shading_retains_unlit_alpha_used_by_cutout_shadows() {
    assert!(SURFACE_SOURCE.contains("let original_shade = i32(packed & 15u);"));
    assert!(SURFACE_SOURCE.contains("vec2<i32>(original_shade, i32(ramp))"));
    assert!(SURFACE_SOURCE.contains("lit_color.a != original_color.a"));
    assert!(SURFACE_SOURCE.contains("return vec4<f32>(rgb, original_color.a);"));
    assert!(SHADOW_CUTOUT_SOURCE.contains("let shade = i32(packed & 15u);"));
}
