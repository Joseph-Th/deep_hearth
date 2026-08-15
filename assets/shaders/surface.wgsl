const DH_SURFACE_TILE_SIZE: u32 = 16u;
const DH_SURFACE_MAX_LIGHTS_PER_TILE: u32 = 32u;

struct SurfaceFrame {
    view_projection: mat4x4<f32>,
    shadow_view_projection: mat4x4<f32>,
    camera_position_time: vec4<f32>,
    sun_direction_intensity: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    fog_color_density: vec4<f32>,
    fog_height_distance: vec4<f32>,
    viewport_size_inverse: vec4<f32>,
    light_grid: vec4<u32>,
}

struct SurfaceVertexInput {
    @location(0) world_position: vec3<f32>,
    @location(1) uv_light: vec4<f32>,
    @location(2) normal_ao: vec4<f32>,
    @location(3) texture_key: u32,
    @location(4) tint: vec4<f32>,
}

struct SurfaceVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal_ao: vec4<f32>,
    @location(3) light: vec2<f32>,
    @location(4) @interpolate(flat) texture_key: u32,
    @location(5) tint: vec4<f32>,
    @location(6) shadow_position: vec4<f32>,
}

@group(0) @binding(0) var<uniform> surface_frame: SurfaceFrame;
@group(0) @binding(1) var surface_shadow_map: texture_depth_2d;
@group(0) @binding(2) var surface_shadow_sampler: sampler_comparison;
@group(1) @binding(0) var surface_indexed_textures: texture_2d_array<u32>;
@group(1) @binding(1) var surface_palette_rows: texture_2d<u32>;
@group(1) @binding(2) var surface_palette_colors: texture_2d<f32>;
@group(2) @binding(0) var<storage, read> surface_point_lights: array<DhPointLight>;
@group(2) @binding(1) var<storage, read> surface_tile_light_counts: array<u32>;
@group(2) @binding(2) var<storage, read> surface_tile_light_indices: array<u32>;

@vertex
fn surface_vs(input: SurfaceVertexInput) -> SurfaceVertexOutput {
    var output: SurfaceVertexOutput;
    let world_position = vec4<f32>(input.world_position, 1.0);
    output.position = surface_frame.view_projection * world_position;
    output.world_position = input.world_position;
    output.uv = input.uv_light.xy;
    output.normal_ao = input.normal_ao;
    output.light = input.uv_light.zw;
    output.texture_key = input.texture_key;
    output.tint = input.tint;
    output.shadow_position = surface_frame.shadow_view_projection * world_position;
    return output;
}

fn surface_palette_sample(uv: vec2<f32>, texture_key: u32, shade_delta: i32) -> vec4<f32> {
    let layer = i32(texture_key & 0xffffu);
    let palette_row = i32(texture_key >> 16u);
    let mip_level = dh_indexed_texture_mip_level(uv);
    let texel_coordinate = dh_indexed_texture_coordinate(uv, mip_level);
    let packed = textureLoad(
        surface_indexed_textures,
        texel_coordinate,
        layer,
        mip_level,
    ).x;
    let palette_slot = i32(packed >> 4u);
    let shade = clamp(i32(packed & 15u) + shade_delta, 0, 15);
    let ramp = textureLoad(
        surface_palette_rows,
        vec2<i32>(palette_slot, palette_row),
        0,
    ).x;
    return textureLoad(
        surface_palette_colors,
        vec2<i32>(shade, i32(ramp)),
        0,
    );
}

fn surface_shadow_visibility(shadow_position: vec4<f32>, normal: vec3<f32>) -> f32 {
    let projected = shadow_position.xyz / max(abs(shadow_position.w), DH_EPSILON);
    let uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || projected.z >= 1.0) {
        return 1.0;
    }
    let dimensions = vec2<f32>(textureDimensions(surface_shadow_map));
    let texel = vec2<f32>(1.0) / dimensions;
    let light_direction = dh_safe_normalize(-surface_frame.sun_direction_intensity.xyz);
    let bias = mix(0.0015, 0.00025, dh_saturate(dot(normal, light_direction)));
    let comparison_depth = projected.z - bias;
    let first = textureSampleCompare(
        surface_shadow_map,
        surface_shadow_sampler,
        uv + vec2<f32>(-0.5, -0.5) * texel,
        comparison_depth,
    );
    let second = textureSampleCompare(
        surface_shadow_map,
        surface_shadow_sampler,
        uv + vec2<f32>(0.5, -0.5) * texel,
        comparison_depth,
    );
    let third = textureSampleCompare(
        surface_shadow_map,
        surface_shadow_sampler,
        uv + vec2<f32>(-0.5, 0.5) * texel,
        comparison_depth,
    );
    let fourth = textureSampleCompare(
        surface_shadow_map,
        surface_shadow_sampler,
        uv + vec2<f32>(0.5, 0.5) * texel,
        comparison_depth,
    );
    return (first + second + third + fourth) * 0.25;
}

fn surface_local_radiance(
    fragment_position: vec4<f32>,
    world_position: vec3<f32>,
    normal: vec3<f32>,
    view_direction: vec3<f32>,
) -> vec3<f32> {
    let tile_coordinate = vec2<u32>(fragment_position.xy) / DH_SURFACE_TILE_SIZE;
    let tile_index = tile_coordinate.y * surface_frame.light_grid.x + tile_coordinate.x;
    let tile_count = min(
        surface_tile_light_counts[tile_index],
        DH_SURFACE_MAX_LIGHTS_PER_TILE,
    );
    let available_lights = min(surface_frame.light_grid.z, arrayLength(&surface_point_lights));
    var radiance = vec3<f32>(0.0);
    var local_index = 0u;
    loop {
        if (local_index >= tile_count || local_index >= DH_SURFACE_MAX_LIGHTS_PER_TILE) {
            break;
        }
        let light_index = surface_tile_light_indices[
            tile_index * DH_SURFACE_MAX_LIGHTS_PER_TILE + local_index
        ];
        if (light_index < available_lights) {
            radiance += dh_point_radiance(
                world_position,
                normal,
                view_direction,
                0.78,
                surface_point_lights[light_index],
            );
        }
        local_index += 1u;
    }
    return radiance;
}

@fragment
fn surface_fs(input: SurfaceVertexOutput) -> @location(0) vec4<f32> {
    let normal = dh_safe_normalize(input.normal_ao.xyz);
    let ambient_occlusion = dh_saturate(input.normal_ao.w);
    let sky_light = dh_saturate(input.light.x);
    let block_light = dh_saturate(input.light.y);
    let ramp_light = max(sky_light * surface_frame.sun_direction_intensity.w, block_light);
    let shade_delta = i32(round((ramp_light - 0.5) * 5.0));
    let base = surface_palette_sample(input.uv, input.texture_key, shade_delta) * input.tint;
    if (base.a < 0.01) {
        discard;
    }

    let view_direction = dh_safe_normalize(
        surface_frame.camera_position_time.xyz - input.world_position,
    );
    var shadow = 1.0;
    if (sky_light > 0.001 && surface_frame.sun_direction_intensity.w > 0.001) {
        shadow = surface_shadow_visibility(input.shadow_position, normal);
    }
    let sun = DhDirectionalLight(
        surface_frame.sun_direction_intensity,
        surface_frame.sun_color,
    );
    let sun_radiance = dh_directional_radiance(normal, view_direction, 0.82, sun) * shadow * sky_light;
    let sky_ambient = dh_sky_ambient(normal, surface_frame.sky_color.rgb) *
        mix(0.12, 1.0, sky_light) * ambient_occlusion;
    let block_radiance = vec3<f32>(1.0, 0.46, 0.15) * dh_square(block_light) * 1.35;
    let local_radiance = surface_local_radiance(
        input.position,
        input.world_position,
        normal,
        view_direction,
    );
    var color = base.rgb * (sky_ambient + sun_radiance + block_radiance + local_radiance);
    let fog = dh_height_fog_amount(
        input.world_position,
        surface_frame.camera_position_time.xyz,
        surface_frame.fog_color_density.w,
        surface_frame.fog_height_distance.x,
        surface_frame.fog_height_distance.y,
        surface_frame.fog_height_distance.z,
    );
    color = mix(color, surface_frame.fog_color_density.rgb, fog);
    return vec4<f32>(color, base.a);
}
