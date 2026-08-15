struct DhDirectionalLight {
    direction_intensity: vec4<f32>,
    color: vec4<f32>,
}

struct DhPointLight {
    position_radius: vec4<f32>,
    color_intensity: vec4<f32>,
}

fn dh_directional_radiance(
    normal: vec3<f32>,
    view_direction: vec3<f32>,
    roughness: f32,
    light: DhDirectionalLight,
) -> vec3<f32> {
    let light_direction = dh_safe_normalize(-light.direction_intensity.xyz);
    let half_direction = dh_safe_normalize(light_direction + view_direction);
    let diffuse = dh_saturate(dot(normal, light_direction));
    let specular_power = mix(64.0, 4.0, dh_saturate(roughness));
    let specular = pow(dh_saturate(dot(normal, half_direction)), specular_power) *
        (1.0 - roughness) * 0.18;
    return light.color.rgb * light.direction_intensity.w * (diffuse + specular);
}

fn dh_point_radiance(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    view_direction: vec3<f32>,
    roughness: f32,
    light: DhPointLight,
) -> vec3<f32> {
    let to_light = light.position_radius.xyz - world_position;
    let distance_squared = max(dot(to_light, to_light), DH_EPSILON);
    let distance = sqrt(distance_squared);
    let radius = max(light.position_radius.w, 0.001);
    let range_fade = dh_square(dh_saturate(1.0 - distance / radius));
    let attenuation = range_fade / (1.0 + distance_squared * 0.08);
    let light_direction = to_light / distance;
    let half_direction = dh_safe_normalize(light_direction + view_direction);
    let diffuse = dh_saturate(dot(normal, light_direction));
    let specular_power = mix(48.0, 4.0, dh_saturate(roughness));
    let specular = pow(dh_saturate(dot(normal, half_direction)), specular_power) *
        (1.0 - roughness) * 0.12;
    return light.color_intensity.rgb * light.color_intensity.w * attenuation *
        (diffuse + specular);
}

fn dh_sky_ambient(normal: vec3<f32>, sky_color: vec3<f32>) -> vec3<f32> {
    let sky_visibility = normal.y * 0.5 + 0.5;
    let ground_bounce = vec3<f32>(0.10, 0.075, 0.055);
    return mix(ground_bounce, sky_color, sky_visibility);
}

fn dh_height_fog_amount(
    world_position: vec3<f32>,
    camera_position: vec3<f32>,
    density: f32,
    base_height: f32,
    height_falloff: f32,
    maximum_distance: f32,
) -> f32 {
    let distance = min(length(world_position - camera_position), maximum_distance);
    let sample_height = (world_position.y + camera_position.y) * 0.5;
    let height_density = exp(-max(sample_height - base_height, 0.0) * height_falloff);
    return 1.0 - exp(-distance * density * height_density);
}
