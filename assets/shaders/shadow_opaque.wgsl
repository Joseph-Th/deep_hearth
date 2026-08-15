struct OpaqueShadowFrame {
    view_projection: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> opaque_shadow_frame: OpaqueShadowFrame;

@vertex
fn shadow_opaque_vs(
    @location(0) world_position: vec3<f32>,
) -> @builtin(position) vec4<f32> {
    return opaque_shadow_frame.view_projection * vec4<f32>(world_position, 1.0);
}
