fn dh_hash_u32(input: u32) -> u32 {
    var value = input;
    value = value ^ (value >> 16u);
    value = value * 0x7feb352du;
    value = value ^ (value >> 15u);
    value = value * 0x846ca68bu;
    return value ^ (value >> 16u);
}

fn dh_hash_cell_2d(cell: vec2<i32>) -> f32 {
    let x = bitcast<u32>(cell.x);
    let y = bitcast<u32>(cell.y);
    let mixed = dh_hash_u32(x ^ (dh_hash_u32(y) + 0x9e3779b9u));
    return f32(mixed & 0x00ffffffu) * (1.0 / 16777216.0);
}

fn dh_value_noise_2d(position: vec2<f32>) -> f32 {
    let cell = vec2<i32>(floor(position));
    let local = fract(position);
    let blend = local * local * (vec2<f32>(3.0) - 2.0 * local);
    let lower_left = dh_hash_cell_2d(cell);
    let lower_right = dh_hash_cell_2d(cell + vec2<i32>(1, 0));
    let upper_left = dh_hash_cell_2d(cell + vec2<i32>(0, 1));
    let upper_right = dh_hash_cell_2d(cell + vec2<i32>(1, 1));
    let lower = mix(lower_left, lower_right, blend.x);
    let upper = mix(upper_left, upper_right, blend.x);
    return mix(lower, upper, blend.y);
}

// Exactly three layers: twelve scalar hash evaluations and no data-dependent loop.
fn dh_fbm_2d_3(position: vec2<f32>) -> f32 {
    let first = dh_value_noise_2d(position);
    let second_position = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * position + vec2<f32>(17.1, 9.2);
    let second = dh_value_noise_2d(second_position);
    let third_position = mat2x2<f32>(1.7, -1.1, 1.1, 1.7) * second_position + vec2<f32>(8.3, 23.7);
    let third = dh_value_noise_2d(third_position);
    return first * 0.5714286 + second * 0.2857143 + third * 0.1428571;
}
