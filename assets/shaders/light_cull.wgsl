const DH_CULL_TILE_SIZE: u32 = 16u;
const DH_CULL_MAX_LIGHTS_PER_TILE: u32 = 32u;
const DH_CULL_MAX_VISIBLE_LIGHTS: u32 = 512u;
const DH_CULL_WORKGROUP_SIZE: u32 = 64u;
const DH_CULL_LIGHTS_PER_LANE: u32 = 8u;

struct LightCullFrame {
    view_projection: mat4x4<f32>,
    camera_right: vec4<f32>,
    viewport_tiles: vec4<u32>,
    light_count: vec4<u32>,
}

@group(0) @binding(0) var<uniform> light_cull_frame: LightCullFrame;
@group(1) @binding(0) var<storage, read> cull_point_lights: array<DhPointLight>;
@group(1) @binding(1) var<storage, read_write> cull_tile_light_counts: array<u32>;
@group(1) @binding(2) var<storage, read_write> cull_tile_light_indices: array<u32>;

var<workgroup> cull_lane_counts: array<u32, 64>;

fn light_overlaps_tile(light: DhPointLight, tile: vec2<u32>) -> bool {
    let center_clip = light_cull_frame.view_projection *
        vec4<f32>(light.position_radius.xyz, 1.0);
    if (center_clip.w <= DH_EPSILON) {
        return false;
    }
    let edge_world = light.position_radius.xyz +
        light_cull_frame.camera_right.xyz * light.position_radius.w;
    let edge_clip = light_cull_frame.view_projection * vec4<f32>(edge_world, 1.0);
    let center_ndc = center_clip.xy / center_clip.w;
    let edge_ndc = edge_clip.xy / max(edge_clip.w, DH_EPSILON);
    let viewport = vec2<f32>(light_cull_frame.viewport_tiles.xy);
    let center_pixel = (center_ndc * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5)) * viewport;
    let edge_pixel = (edge_ndc * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5)) * viewport;
    let radius_pixels = max(length(edge_pixel - center_pixel), 1.0);
    let tile_minimum = vec2<f32>(tile * DH_CULL_TILE_SIZE);
    let tile_maximum = tile_minimum + vec2<f32>(f32(DH_CULL_TILE_SIZE));
    let closest = clamp(center_pixel, tile_minimum, tile_maximum);
    let distance = center_pixel - closest;
    return dot(distance, distance) <= radius_pixels * radius_pixels;
}

@compute @workgroup_size(64, 1, 1)
fn light_cull_cs(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let tile = workgroup_id.xy;
    if (tile.x >= light_cull_frame.viewport_tiles.z ||
        tile.y >= light_cull_frame.viewport_tiles.w) {
        return;
    }
    let available_lights = min(
        min(light_cull_frame.light_count.x, arrayLength(&cull_point_lights)),
        DH_CULL_MAX_VISIBLE_LIGHTS,
    );
    var selected_indices: array<u32, 8>;
    var selected_count = 0u;
    var lane_light_offset = 0u;
    loop {
        if (lane_light_offset >= DH_CULL_LIGHTS_PER_LANE) {
            break;
        }
        let light_index = local_index * DH_CULL_LIGHTS_PER_LANE + lane_light_offset;
        if (light_index < available_lights) {
            let light = cull_point_lights[light_index];
            if (light_overlaps_tile(light, tile)) {
                selected_indices[selected_count] = light_index;
                selected_count += 1u;
            }
        }
        lane_light_offset += 1u;
    }
    cull_lane_counts[local_index] = selected_count;
    workgroupBarrier();

    let tile_index = tile.y * light_cull_frame.viewport_tiles.z + tile.x;
    var lane_output_offset = 0u;
    var previous_lane = 0u;
    loop {
        if (previous_lane >= local_index || lane_output_offset >= DH_CULL_MAX_LIGHTS_PER_TILE) {
            break;
        }
        lane_output_offset += cull_lane_counts[previous_lane];
        previous_lane += 1u;
    }
    if (local_index == 0u) {
        var total_count = 0u;
        var lane = 0u;
        loop {
            if (lane >= DH_CULL_WORKGROUP_SIZE || total_count >= DH_CULL_MAX_LIGHTS_PER_TILE) {
                break;
            }
            total_count += cull_lane_counts[lane];
            lane += 1u;
        }
        cull_tile_light_counts[tile_index] = min(total_count, DH_CULL_MAX_LIGHTS_PER_TILE);
    }
    var lane_selected_index = 0u;
    loop {
        let output_index = lane_output_offset + lane_selected_index;
        if (lane_selected_index >= selected_count || output_index >= DH_CULL_MAX_LIGHTS_PER_TILE) {
            break;
        }
        cull_tile_light_indices[
            tile_index * DH_CULL_MAX_LIGHTS_PER_TILE + output_index
        ] = selected_indices[lane_selected_index];
        lane_selected_index += 1u;
    }
}
