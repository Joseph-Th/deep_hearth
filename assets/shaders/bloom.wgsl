struct BloomFrame {
    source_inverse_size: vec4<f32>,
    output_size: vec4<u32>,
    threshold_knee: vec4<f32>,
}

@group(0) @binding(0) var<uniform> bloom_frame: BloomFrame;
@group(1) @binding(0) var bloom_source: texture_2d<f32>;
@group(1) @binding(1) var bloom_linear_sampler: sampler;
@group(1) @binding(2) var bloom_output: texture_storage_2d<rgba16float, write>;

fn bloom_brightness_weight(color: vec3<f32>) -> f32 {
    let brightness = max(max(color.r, color.g), color.b);
    let threshold = max(bloom_frame.threshold_knee.x, 0.0);
    let knee = max(bloom_frame.threshold_knee.y, 0.0001);
    let soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    let soft_contribution = soft * soft * (0.25 / knee);
    return max(brightness - threshold, soft_contribution) / max(brightness, DH_EPSILON);
}

@compute @workgroup_size(8, 8, 1)
fn bloom_downsample_cs(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    if (invocation_id.x >= bloom_frame.output_size.x ||
        invocation_id.y >= bloom_frame.output_size.y) {
        return;
    }
    let output_size = vec2<f32>(bloom_frame.output_size.xy);
    let uv = (vec2<f32>(invocation_id.xy) + vec2<f32>(0.5)) / output_size;
    let offset = bloom_frame.source_inverse_size.xy * 0.75;
    let first = textureSampleLevel(
        bloom_source,
        bloom_linear_sampler,
        uv + vec2<f32>(-offset.x, -offset.y),
        0.0,
    ).rgb;
    let second = textureSampleLevel(
        bloom_source,
        bloom_linear_sampler,
        uv + vec2<f32>(offset.x, -offset.y),
        0.0,
    ).rgb;
    let third = textureSampleLevel(
        bloom_source,
        bloom_linear_sampler,
        uv + vec2<f32>(-offset.x, offset.y),
        0.0,
    ).rgb;
    let fourth = textureSampleLevel(
        bloom_source,
        bloom_linear_sampler,
        uv + vec2<f32>(offset.x, offset.y),
        0.0,
    ).rgb;
    let color = (first + second + third + fourth) * 0.25;
    textureStore(
        bloom_output,
        vec2<i32>(invocation_id.xy),
        vec4<f32>(color * bloom_brightness_weight(color), 1.0),
    );
}
