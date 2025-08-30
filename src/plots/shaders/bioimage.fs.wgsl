struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

// The texture is bound as a read-only storage texture.
// Even though the data is 16-bit, WGSL promotes integer texture formats to 32-bit integers
// (u32 for unsigned, i32 for signed) when they are read in the shader.
@group(0) @binding(1) var img_tex: texture_2d<u32>;

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Get texture dimensions to convert normalized coordinates to pixel coordinates.
    let tex_dims = vec2<f32>(textureDimensions(img_tex));

    // Calculate integer pixel coordinates from normalized texture coordinates.
    // We need to clamp to avoid reading out of bounds if tex_coord is exactly 1.0.
    let texel_coords = vec2<i32>(
        min(
            floor(in.tex_coord * tex_dims),
            tex_dims - vec2<f32>(1.0, 1.0)
        )
    );

    // Load the texel value. Since it's a u32 texture, we use textureLoad.
    // We assume the data is in the first channel (.r).
    let intensity_u32 = textureLoad(img_tex, texel_coords, 0).r;

    // Normalize the intensity to a 0.0-1.0 float.
    // This assumes the input data is 16-bit (max value 65535).
    // Adjust the normalization factor if your data has a different bit depth.
    let intensity_f32 = f32(intensity_u32) / 65535.0;

    // Output a grayscale color.
    return vec4<f32>(intensity_f32, intensity_f32, intensity_f32, 1.0);
}