struct PostProcessFrame {
    exposure_bloom: vec4<f32>,
    color_grade: vec4<f32>,
    vignette_dither: vec4<f32>,
    viewport_frame: vec4<u32>,
}

struct PostVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var<uniform> post_frame: PostProcessFrame;
@group(1) @binding(0) var post_hdr_color: texture_2d<f32>;
@group(1) @binding(1) var post_bloom_color: texture_2d<f32>;
@group(1) @binding(2) var post_linear_sampler: sampler;

@vertex
fn post_process_vs(@builtin(vertex_index) vertex_index: u32) -> PostVertexOutput {
    var output: PostVertexOutput;
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    output.uv = vec2<f32>(x, y);
    output.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return output;
}

@fragment
fn post_process_fs(input: PostVertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSampleLevel(post_hdr_color, post_linear_sampler, input.uv, 0.0).rgb;
    let bloom_offset = post_frame.exposure_bloom.zw;
    let bloom = (
        textureSampleLevel(
            post_bloom_color,
            post_linear_sampler,
            input.uv + vec2<f32>(-bloom_offset.x, -bloom_offset.y),
            0.0,
        ).rgb +
        textureSampleLevel(
            post_bloom_color,
            post_linear_sampler,
            input.uv + vec2<f32>(bloom_offset.x, -bloom_offset.y),
            0.0,
        ).rgb +
        textureSampleLevel(
            post_bloom_color,
            post_linear_sampler,
            input.uv + vec2<f32>(-bloom_offset.x, bloom_offset.y),
            0.0,
        ).rgb +
        textureSampleLevel(
            post_bloom_color,
            post_linear_sampler,
            input.uv + vec2<f32>(bloom_offset.x, bloom_offset.y),
            0.0,
        ).rgb
    ) * 0.25;
    var color = (hdr + bloom * post_frame.exposure_bloom.y) * post_frame.exposure_bloom.x;
    color = dh_aces_fitted(color);
    let luminance = dh_luminance(color);
    color = mix(vec3<f32>(luminance), color, post_frame.color_grade.x);
    color = (color - vec3<f32>(0.5)) * post_frame.color_grade.y + vec3<f32>(0.5);
    color *= post_frame.color_grade.z;

    let centered = input.uv * 2.0 - vec2<f32>(1.0);
    let vignette = dh_saturate(1.0 - dot(centered, centered) * post_frame.vignette_dither.x);
    color *= mix(1.0, vignette, post_frame.vignette_dither.y);
    let dither = dh_interleaved_gradient_noise(
        input.position.xy,
        post_frame.viewport_frame.z,
    ) - 0.5;
    color += vec3<f32>(dither * post_frame.vignette_dither.z * (1.0 / 255.0));
    return vec4<f32>(dh_saturate_vec3(color), 1.0);
}
