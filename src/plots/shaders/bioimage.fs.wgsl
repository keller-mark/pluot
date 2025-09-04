struct Uniforms {
    camera_view: mat4x4<f32>,
    viewport_size: vec2<f32>, // (width, height) in pixels

    num_channels: u32,
    _pad0: f32,
    
    // See "runtime sized arrays" info
    // Reference: https://webgpufundamentals.org/webgpu/lessons/webgpu-wgsl.html#runtime-sized-arrays
    channel_windows: array<vec4<f32>, 8>,
    channel_colors: array<vec4<f32>, 8>,
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

// The texture is bound as a read-only storage texture.
// Even though the data is 16-bit, WGSL promotes integer texture formats to 32-bit integers
// (u32 for unsigned, i32 for signed) when they are read in the shader.
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d_array<u32>;

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
    // The fourth argument to textureLoad is the mip level, which is 0 for us.
    let ch0_intensity_u32 = textureLoad(img_tex, texel_coords, 0, 0).r;
    let ch1_intensity_u32 = textureLoad(img_tex, texel_coords, 1, 0).r;

    let ch0_color = u.channel_colors[0]; // Color for channel 0
    let ch1_color = u.channel_colors[1]; // Color for channel 1

    let ch0_window = u.channel_windows[0]; // Window for channel 0
    let ch1_window = u.channel_windows[1]; // Window for channel 1

    // Normalize the intensity to a 0.0-1.0 float.
    // This assumes the input data is 16-bit (max value 65535).
    // Adjust the normalization factor if your data has a different bit depth.
    let ch0_intensity_f32 = f32(ch0_intensity_u32) / 65535.0;
    let ch1_intensity_f32 = f32(ch1_intensity_u32) / 65535.0;

    // Apply windowing to adjust contrast limits.
    // Reference: https://github.com/hms-dbmi/viv/blob/08a74203b99f54bc62307c741944ed61e33e810c/packages/layers/src/xr-layer/shader-modules/channel-intensity.js#L2
    let ch0_windowed = clamp((ch0_intensity_f32 - ch0_window.x) / max(0.0005, (ch0_window.y - ch0_window.x)), 0.0, 1.0);
    let ch1_windowed = clamp((ch1_intensity_f32 - ch1_window.x) / max(0.0005, (ch1_window.y - ch1_window.x)), 0.0, 1.0);

    // Additively blend the colors based on their intensity.
    // References:
    // - https://github.com/hms-dbmi/viv/blob/main/packages/extensions/src/color-palette-extension/color-palette-module.js
    // - https://github.com/hms-dbmi/viv/blob/08a74203b99f54bc62307c741944ed61e33e810c/packages/layers/src/xr-layer/xr-layer-fragment.glsl.js#L39
    let final_color = ch0_color * ch0_windowed + ch1_color * ch1_windowed;

    // Output the blended color.
    return vec4<f32>(final_color.xyz, 1.0);
}