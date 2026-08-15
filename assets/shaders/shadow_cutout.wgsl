struct CutoutShadowFrame {
    view_projection: mat4x4<f32>,
}

struct ShadowVertexInput {
    @location(0) world_position: vec3<f32>,
    @location(1) uv_light: vec4<f32>,
    @location(3) texture_key: u32,
}

struct ShadowVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) texture_key: u32,
}

@group(0) @binding(0) var<uniform> cutout_shadow_frame: CutoutShadowFrame;
@group(1) @binding(0) var shadow_indexed_textures: texture_2d_array<u32>;
@group(1) @binding(1) var shadow_palette_rows: texture_2d<u32>;
@group(1) @binding(2) var shadow_palette_colors: texture_2d<f32>;

@vertex
fn shadow_cutout_vs(input: ShadowVertexInput) -> ShadowVertexOutput {
    var output: ShadowVertexOutput;
    output.position = cutout_shadow_frame.view_projection * vec4<f32>(input.world_position, 1.0);
    output.uv = input.uv_light.xy;
    output.texture_key = input.texture_key;
    return output;
}

@fragment
fn shadow_cutout_fs(input: ShadowVertexOutput) {
    let layer = i32(input.texture_key & 0xffffu);
    let palette_row = i32(input.texture_key >> 16u);
    let mip_level = dh_indexed_texture_mip_level(input.uv);
    let coordinate = dh_indexed_texture_coordinate(input.uv, mip_level);
    let packed = textureLoad(
        shadow_indexed_textures,
        coordinate,
        layer,
        mip_level,
    ).x;
    let palette_slot = i32(packed >> 4u);
    let shade = i32(packed & 15u);
    let ramp = textureLoad(
        shadow_palette_rows,
        vec2<i32>(palette_slot, palette_row),
        0,
    ).x;
    let alpha = textureLoad(
        shadow_palette_colors,
        vec2<i32>(shade, i32(ramp)),
        0,
    ).a;
    if (alpha < 0.5) {
        discard;
    }
}
