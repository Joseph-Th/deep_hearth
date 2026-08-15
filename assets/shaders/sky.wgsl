struct SkyFrame {
    inverse_view_projection: mat4x4<f32>,
    camera_position_time: vec4<f32>,
    sun_direction_intensity: vec4<f32>,
    sun_color: vec4<f32>,
    zenith_color: vec4<f32>,
    horizon_color: vec4<f32>,
    cloud_coverage_speed: vec4<f32>,
    night_color_strength: vec4<f32>,
}

struct SkyVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var<uniform> sky_frame: SkyFrame;

@vertex
fn sky_vs(@builtin(vertex_index) vertex_index: u32) -> SkyVertexOutput {
    var output: SkyVertexOutput;
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    output.uv = vec2<f32>(x, y);
    output.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 1.0, 1.0);
    return output;
}

@fragment
fn sky_fs(input: SkyVertexOutput) -> @location(0) vec4<f32> {
    let far_world = dh_world_position_from_depth(
        input.uv,
        1.0,
        sky_frame.inverse_view_projection,
    );
    let view_direction = dh_safe_normalize(far_world - sky_frame.camera_position_time.xyz);
    let elevation = dh_saturate(view_direction.y * 0.5 + 0.5);
    let gradient = pow(elevation, 0.38);
    var color = mix(sky_frame.horizon_color.rgb, sky_frame.zenith_color.rgb, gradient);

    let sun_direction = dh_safe_normalize(-sky_frame.sun_direction_intensity.xyz);
    let sun_alignment = dh_saturate(dot(view_direction, sun_direction));
    let sun_disc = smoothstep(0.99955, 0.99992, sun_alignment);
    let sun_glow = pow(sun_alignment, 96.0) * 0.35;
    color += sky_frame.sun_color.rgb * sky_frame.sun_direction_intensity.w * (sun_disc + sun_glow);

    if (view_direction.y > 0.02) {
        let cloud_position = view_direction.xz / max(view_direction.y + 0.18, 0.20) * 1.7 +
            sky_frame.camera_position_time.w * sky_frame.cloud_coverage_speed.yz;
        let cloud_noise = dh_fbm_2d_3(cloud_position);
        let cloud_density = smoothstep(
            sky_frame.cloud_coverage_speed.x,
            min(sky_frame.cloud_coverage_speed.x + 0.18, 0.98),
            cloud_noise,
        ) * smoothstep(0.02, 0.22, view_direction.y);
        let cloud_light = mix(
            sky_frame.horizon_color.rgb * 0.72,
            sky_frame.sun_color.rgb,
            dh_saturate(sun_direction.y * 0.5 + 0.5),
        );
        color = mix(color, cloud_light, cloud_density * sky_frame.cloud_coverage_speed.w);
    }

    let night_strength = dh_saturate(sky_frame.night_color_strength.w);
    if (night_strength > 0.001) {
        let star_cell = vec2<i32>(floor(view_direction.xz / max(view_direction.y + 1.1, 0.1) * 420.0));
        let star_hash = dh_hash_cell_2d(star_cell);
        let star = smoothstep(0.9965, 0.9998, star_hash) *
            (0.65 + 0.35 * sin(sky_frame.camera_position_time.w * 2.0 + star_hash * 37.0));
        color = mix(color, sky_frame.night_color_strength.rgb, night_strength * 0.72);
        color += vec3<f32>(star * night_strength * smoothstep(0.0, 0.35, view_direction.y));
    }
    return vec4<f32>(color, 1.0);
}
