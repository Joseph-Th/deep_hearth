struct WaterFrame {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    camera_position_time: vec4<f32>,
    sun_direction_intensity: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    fog_color_density: vec4<f32>,
    fog_height_distance: vec4<f32>,
    viewport_size_inverse: vec4<f32>,
    absorption_roughness: vec4<f32>,
    scatter_refraction: vec4<f32>,
    wave_scale_speed: vec4<f32>,
}

struct WaterVertexInput {
    @location(0) world_position: vec3<f32>,
    @location(1) color: vec4<f32>,
}

struct WaterVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) wave_crest: f32,
}

@group(0) @binding(0) var<uniform> water_frame: WaterFrame;
@group(1) @binding(0) var water_scene_color: texture_2d<f32>;
@group(1) @binding(1) var water_scene_sampler: sampler;
@group(1) @binding(2) var water_scene_depth: texture_depth_2d;

fn water_wave(position: vec2<f32>, time: f32) -> vec3<f32> {
    let scale = water_frame.wave_scale_speed.x;
    let speed = water_frame.wave_scale_speed.y;
    let amplitude = water_frame.wave_scale_speed.z;
    let first_phase = dot(position, vec2<f32>(0.894, 0.447)) * scale + time * speed;
    let second_phase = dot(position, vec2<f32>(-0.351, 0.936)) * scale * 1.73 - time * speed * 1.31;
    let third_phase = dot(position, vec2<f32>(0.196, -0.981)) * scale * 2.41 + time * speed * 0.73;
    let height = (sin(first_phase) * 0.52 + sin(second_phase) * 0.31 +
        sin(third_phase) * 0.17) * amplitude;
    let slope_x = (cos(first_phase) * 0.894 * scale * 0.52 +
        cos(second_phase) * -0.351 * scale * 1.73 * 0.31 +
        cos(third_phase) * 0.196 * scale * 2.41 * 0.17) * amplitude;
    let slope_z = (cos(first_phase) * 0.447 * scale * 0.52 +
        cos(second_phase) * 0.936 * scale * 1.73 * 0.31 +
        cos(third_phase) * -0.981 * scale * 2.41 * 0.17) * amplitude;
    return vec3<f32>(height, slope_x, slope_z);
}

@vertex
fn water_vs(input: WaterVertexInput) -> WaterVertexOutput {
    var output: WaterVertexOutput;
    let wave = water_wave(input.world_position.xz, water_frame.camera_position_time.w);
    let displaced = input.world_position + vec3<f32>(0.0, wave.x, 0.0);
    output.position = water_frame.view_projection * vec4<f32>(displaced, 1.0);
    output.world_position = displaced;
    output.normal = dh_safe_normalize(vec3<f32>(-wave.y, 1.0, -wave.z));
    output.color = input.color;
    output.wave_crest = dh_saturate(wave.x / max(water_frame.wave_scale_speed.z, 0.001) * 0.5 + 0.5);
    return output;
}

@fragment
fn water_fs(input: WaterVertexOutput) -> @location(0) vec4<f32> {
    let inverse_viewport = water_frame.viewport_size_inverse.zw;
    let screen_uv = dh_screen_uv(input.position, inverse_viewport);
    let viewport_size = vec2<i32>(water_frame.viewport_size_inverse.xy);
    let depth_coordinate = clamp(
        vec2<i32>(input.position.xy),
        vec2<i32>(0),
        viewport_size - vec2<i32>(1),
    );
    let scene_depth = textureLoad(water_scene_depth, depth_coordinate, 0);
    let scene_world = dh_world_position_from_depth(
        screen_uv,
        scene_depth,
        water_frame.inverse_view_projection,
    );
    let camera = water_frame.camera_position_time.xyz;
    let water_distance = length(input.world_position - camera);
    let scene_distance = length(scene_world - camera);
    let thickness = clamp(scene_distance - water_distance, 0.0, 24.0);
    let normal = dh_safe_normalize(input.normal);
    let view_direction = dh_safe_normalize(camera - input.world_position);
    let edge_fade = dh_saturate(min(min(screen_uv.x, 1.0 - screen_uv.x),
        min(screen_uv.y, 1.0 - screen_uv.y)) * 30.0);
    let refraction_offset = normal.xz * water_frame.scatter_refraction.w *
        dh_saturate(thickness * 0.25) * edge_fade;
    let refracted = textureSampleLevel(
        water_scene_color,
        water_scene_sampler,
        clamp(screen_uv + refraction_offset, vec2<f32>(0.001), vec2<f32>(0.999)),
        0.0,
    ).rgb;

    let transmittance = exp(-water_frame.absorption_roughness.rgb * thickness);
    let scattered = water_frame.scatter_refraction.rgb * (vec3<f32>(1.0) - transmittance);
    let fresnel = 0.02 + 0.98 * pow(1.0 - dh_saturate(dot(normal, view_direction)), 5.0);
    let sky_reflection = mix(
        water_frame.fog_color_density.rgb,
        water_frame.sky_color.rgb,
        dh_saturate(normal.y * 0.75 + 0.25),
    );
    let reflected_sun = pow(
        dh_saturate(dot(reflect(-view_direction, normal),
            dh_safe_normalize(-water_frame.sun_direction_intensity.xyz))),
        mix(96.0, 12.0, water_frame.absorption_roughness.w),
    ) * water_frame.sun_color.rgb * water_frame.sun_direction_intensity.w;
    var color = mix(refracted * transmittance + scattered, sky_reflection + reflected_sun, fresnel);

    let foam_noise = dh_value_noise_2d(
        input.world_position.xz * 0.43 + water_frame.camera_position_time.w * vec2<f32>(0.035, -0.027),
    );
    let shore_foam = 1.0 - smoothstep(0.06, 1.35, thickness);
    let crest_foam = smoothstep(0.78, 0.96, input.wave_crest + (foam_noise - 0.5) * 0.25);
    let foam = dh_saturate(shore_foam * (0.65 + foam_noise * 0.35) + crest_foam * 0.45);
    color = mix(color, vec3<f32>(0.82, 0.91, 0.91), foam * 0.82);
    color *= input.color.rgb;

    let fog = dh_height_fog_amount(
        input.world_position,
        camera,
        water_frame.fog_color_density.w,
        water_frame.fog_height_distance.x,
        water_frame.fog_height_distance.y,
        water_frame.fog_height_distance.z,
    );
    color = mix(color, water_frame.fog_color_density.rgb, fog);
    let alpha = clamp(0.46 + thickness * 0.16 + fresnel * 0.22 + foam * 0.18, 0.46, 0.96) * input.color.a;
    return vec4<f32>(color * alpha, alpha);
}
