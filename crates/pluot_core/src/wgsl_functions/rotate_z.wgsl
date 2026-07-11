fn rotate_z(angle_deg: f32) -> mat4x4<f32> {
    let angle_rad = (-1.0 * angle_deg) * 3.14159265359 / 180.0; // Convert degrees to radians
    let c = cos(angle_rad);
    let s = sin(angle_rad);
    return mat4x4<f32>(
        vec4<f32>(c, s, 0.0, 0.0),
        vec4<f32>(-s, c, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0)
    );
}
