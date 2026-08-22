//! Tests for the sibling definitions module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn shader_dependencies_must_resolve_to_acyclic_libraries() {
    let library_a =
        ShaderDefinition::new_library(ShaderId::new(1), "a", vec![ShaderId::new(2)], "fn a() {}");
    let library_b =
        ShaderDefinition::new_library(ShaderId::new(2), "b", vec![ShaderId::new(1)], "fn b() {}");

    let result = std::panic::catch_unwind(|| ShaderRegistry::new([library_a, library_b]));

    assert!(result.is_err());
}

#[test]
fn work_budget_rejects_unbounded_fragment_light_counts() {
    let result = std::panic::catch_unwind(|| ShaderWorkBudget::new(1, 1, 33, 1));

    assert!(result.is_err());
}

#[test]
fn compute_workgroup_stays_within_portable_invocation_limit() {
    let result = std::panic::catch_unwind(|| ComputeEntryPoint::new("too_wide", [1_025, 1, 1]));

    assert!(result.is_err());
}

#[test]
fn colorless_pipeline_rejects_blending_without_a_color_attachment() {
    let result = std::panic::catch_unwind(|| {
        RenderPipelineProfile::new(
            ShaderBlendMode::PremultipliedAlpha,
            ShaderDepthMode::ReadWrite,
            ShaderColorTarget::None,
        )
    });

    assert!(result.is_err());
}

#[test]
fn color_pipeline_rejects_a_missing_fragment_entry_point() {
    let result = std::panic::catch_unwind(|| {
        ShaderDefinition::new_render(
            ShaderId::new(1),
            "invalid color pipeline",
            Vec::new(),
            "@vertex fn vertex_only() -> @builtin(position) vec4<f32> { return vec4<f32>(); }",
            RenderEntryPoints::new_vertex_only("vertex_only"),
            RenderPipelineProfile::new(
                ShaderBlendMode::Opaque,
                ShaderDepthMode::ReadWrite,
                ShaderColorTarget::LinearHdr,
            ),
            ShaderWorkBudget::new(0, 0, 0, 0),
        )
    });

    assert!(result.is_err());
}
