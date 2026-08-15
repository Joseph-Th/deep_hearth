const DH_PI: f32 = 3.141592653589793;
const DH_EPSILON: f32 = 0.00001;

fn dh_saturate(value: f32) -> f32 {
    return clamp(value, 0.0, 1.0);
}

fn dh_saturate_vec3(value: vec3<f32>) -> vec3<f32> {
    return clamp(value, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn dh_safe_normalize(value: vec3<f32>) -> vec3<f32> {
    return value * inverseSqrt(max(dot(value, value), DH_EPSILON));
}

fn dh_square(value: f32) -> f32 {
    return value * value;
}

fn dh_luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn dh_rotation_2d(angle: f32) -> mat2x2<f32> {
    let cosine = cos(angle);
    let sine = sin(angle);
    return mat2x2<f32>(cosine, sine, -sine, cosine);
}

fn dh_screen_uv(fragment_position: vec4<f32>, inverse_viewport: vec2<f32>) -> vec2<f32> {
    return fragment_position.xy * inverse_viewport;
}

fn dh_world_position_from_depth(
    uv: vec2<f32>,
    depth: f32,
    inverse_view_projection: mat4x4<f32>,
) -> vec3<f32> {
    let clip = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), depth, 1.0);
    let world = inverse_view_projection * clip;
    return world.xyz / max(abs(world.w), DH_EPSILON);
}

fn dh_linear_view_depth(depth: f32, near_plane: f32, far_plane: f32) -> f32 {
    return (near_plane * far_plane) /
        max(far_plane - depth * (far_plane - near_plane), DH_EPSILON);
}

fn dh_interleaved_gradient_noise(pixel: vec2<f32>, frame: u32) -> f32 {
    let offset = f32(frame & 63u) * 0.06711056;
    return fract(52.9829189 * fract(dot(pixel + offset, vec2<f32>(0.06711056, 0.00583715))));
}

fn dh_aces_fitted(color: vec3<f32>) -> vec3<f32> {
    let numerator = color * (2.51 * color + vec3<f32>(0.03));
    let denominator = color * (2.43 * color + vec3<f32>(0.59)) + vec3<f32>(0.14);
    return dh_saturate_vec3(numerator / denominator);
}
