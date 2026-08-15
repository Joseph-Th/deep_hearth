fn dh_indexed_texture_mip_level(uv: vec2<f32>) -> i32 {
    let texel_scale = uv * f32(DH_INDEXED_TEXTURE_SIDE);
    let derivative_x = dpdx(texel_scale);
    let derivative_y = dpdy(texel_scale);
    let footprint_squared = max(
        dot(derivative_x, derivative_x),
        dot(derivative_y, derivative_y),
    );
    return i32(clamp(
        floor(log2(max(footprint_squared, 1.0)) * 0.5),
        0.0,
        f32(DH_INDEXED_TEXTURE_MAX_MIP),
    ));
}

fn dh_indexed_texture_coordinate(uv: vec2<f32>, mip_level: i32) -> vec2<i32> {
    let mip_side = DH_INDEXED_TEXTURE_SIDE >> u32(mip_level);
    return min(
        vec2<i32>(fract(uv) * f32(mip_side)),
        vec2<i32>(i32(mip_side) - 1),
    );
}
