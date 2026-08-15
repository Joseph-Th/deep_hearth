struct SmokeFrame {
    view_projection: mat4x4<f32>,
    camera_position_time: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    sun_direction_intensity: vec4<f32>,
    sun_color: vec4<f32>,
    fog_color_density: vec4<f32>,
    near_far_viewport: vec4<f32>,
}

struct SmokeVertexInput {
    @location(0) corner: vec2<f32>,
    @location(1) center_age: vec4<f32>,
    @location(2) size_rotation_seed: vec4<f32>,
    @location(3) color: vec4<f32>,
}

struct SmokeVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) age_seed: vec2<f32>,
    @location(3) color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> smoke_frame: SmokeFrame;
@group(1) @binding(0) var smoke_scene_depth: texture_depth_2d;

@vertex
fn smoke_vs(input: SmokeVertexInput) -> SmokeVertexOutput {
    var output: SmokeVertexOutput;
    let age = dh_saturate(input.center_age.w);
    let rotation = input.size_rotation_seed.z + age * (input.size_rotation_seed.w - 0.5) * 1.7;
    let rotated_corner = dh_rotation_2d(rotation) * input.corner;
    let expansion = mix(0.65, 1.35, age);
    let wobble = sin(
        smoke_frame.camera_position_time.w * 1.3 + input.size_rotation_seed.w * 19.0 + age * 4.0,
    ) * age * input.size_rotation_seed.x * 0.16;
    let center = input.center_age.xyz + vec3<f32>(wobble, age * input.size_rotation_seed.y * 0.45, 0.0);
    let world_position = center + smoke_frame.camera_right.xyz * rotated_corner.x *
        input.size_rotation_seed.x * expansion + smoke_frame.camera_up.xyz * rotated_corner.y *
        input.size_rotation_seed.y * expansion;
    output.position = smoke_frame.view_projection * vec4<f32>(world_position, 1.0);
    output.uv = input.corner * 0.5 + vec2<f32>(0.5);
    output.world_position = world_position;
    output.age_seed = vec2<f32>(age, input.size_rotation_seed.w);
    output.color = input.color;
    return output;
}

@fragment
fn smoke_fs(input: SmokeVertexOutput) -> @location(0) vec4<f32> {
    let centered = input.uv * 2.0 - vec2<f32>(1.0);
    let radial = dh_saturate(1.0 - dot(centered, centered));
    let age = input.age_seed.x;
    let seed = input.age_seed.y;
    let age_fade = smoothstep(0.0, 0.10, age) * (1.0 - smoothstep(0.62, 1.0, age));
    if (radial <= 0.001 || age_fade <= 0.001) {
        discard;
    }
    let noise_position = centered * mix(2.1, 4.8, age) +
        vec2<f32>(seed * 31.7, smoke_frame.camera_position_time.w * 0.11 + seed * 7.3);
    let noise = dh_fbm_2d_3(noise_position);
    let billow = smoothstep(0.24, 0.72, noise + radial * 0.38);
    var density = dh_square(radial) * billow * age_fade;

    let viewport_size = vec2<i32>(smoke_frame.near_far_viewport.zw);
    let depth_coordinate = clamp(
        vec2<i32>(input.position.xy),
        vec2<i32>(0),
        viewport_size - vec2<i32>(1),
    );
    let scene_depth = textureLoad(smoke_scene_depth, depth_coordinate, 0);
    let scene_linear = dh_linear_view_depth(
        scene_depth,
        smoke_frame.near_far_viewport.x,
        smoke_frame.near_far_viewport.y,
    );
    let particle_linear = dh_linear_view_depth(
        input.position.z,
        smoke_frame.near_far_viewport.x,
        smoke_frame.near_far_viewport.y,
    );
    density *= dh_saturate((scene_linear - particle_linear) * 0.75);

    let view_direction = dh_safe_normalize(
        smoke_frame.camera_position_time.xyz - input.world_position,
    );
    let sun_direction = dh_safe_normalize(-smoke_frame.sun_direction_intensity.xyz);
    let silver_lining = pow(dh_saturate(dot(view_direction, sun_direction)), 3.0) *
        smoke_frame.sun_direction_intensity.w;
    var color = input.color.rgb * mix(0.42, 0.92, noise) +
        smoke_frame.sun_color.rgb * silver_lining * 0.22;
    let fog_distance = length(input.world_position - smoke_frame.camera_position_time.xyz);
    let fog = 1.0 - exp(-fog_distance * smoke_frame.fog_color_density.w);
    color = mix(color, smoke_frame.fog_color_density.rgb, fog);
    let alpha = density * input.color.a * 0.78;
    return vec4<f32>(color * alpha, alpha);
}
