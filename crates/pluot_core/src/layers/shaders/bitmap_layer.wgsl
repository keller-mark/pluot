fn scale(x: f32, y: f32, z: f32) -> mat4x4<f32> {
  return mat4x4<f32>(
    vec4<f32>(x, 0.0, 0.0, 0.0),
    vec4<f32>(0.0, y, 0.0, 0.0),
    vec4<f32>(0.0, 0.0, z, 0.0),
    vec4<f32>(0.0, 0.0, 0.0, 1.0)
  );
}

fn translate(x: f32, y: f32, z: f32) -> mat4x4<f32> {
  return mat4x4<f32>(
    vec4<f32>(1.0, 0.0, 0.0, 0.0),
    vec4<f32>(0.0, 1.0, 0.0, 0.0),
    vec4<f32>(0.0, 0.0, 1.0, 0.0),
    vec4<f32>(x, y, z, 1.0),
  );
}

fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: u32) -> mat4x4<f32> {
    // Determine the x and y extents to use,
    // based on the aspect ratio mode and layer aspect ratio.
    // We only need to handle the aspect ratio mode when the layer_aspect_ratio is not 1.
    var x_scale_for_aspect_ratio_mode = 1.0;
    var y_scale_for_aspect_ratio_mode = 1.0;
    if (aspect_ratio_mode == 1u) {
        // fit/contain
        if (layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show more than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show exactly (0, 1) in x direction. Show more than (0, 1) in y direction.
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    } else if (aspect_ratio_mode == 2u) {
        // fill/cover
        if(layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show exactly (0, 1) in x direction. Show less than (0, 1) in y direction.
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show less than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    }

    // Only scaling will result in the (0, 1) region being centered.
    // If we want to align 0 to the left or bottom, we need to add a translation step as well.
    // TODO: implement aspect_ratio_alignment_mode
    return scale(
        x_scale_for_aspect_ratio_mode,
        y_scale_for_aspect_ratio_mode,
        1.0
    );
}



struct Channel {
    window: vec2<f32>, // (min, max) for contrast adjustment
    color: vec3<f32>,  // RGB color for the channel
};

struct Uniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode: u32, // 0: pixel units, 1: data units
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end

    img_size: vec2<f32>, // (img_w, img_h) in pixels // TODO: use u32?

    opacity: f32, // Layer opacity

    // Strides for each dimension (in units of f32 elements),
    // allowing the shader to index into the flat data buffer
    // regardless of the dimension ordering (e.g., CYX vs YXC).
    x_stride: u32,
    y_stride: u32,
    c_stride: u32,

    num_channels: u32,

    // See "runtime sized arrays" info
    // Reference: https://webgpufundamentals.org/webgpu/lessons/webgpu-wgsl.html#runtime-sized-arrays
    channels: array<Channel>,
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>, // Pass texture coordinates to fragment shader
};

// The data is converted to f32 on the CPU side (regardless of original dtype)
// and uploaded as a flat storage buffer. The shader uses strides to index
// into the buffer, handling any dimension ordering (e.g., CYX vs YXC).
@group(0) @binding(0) var<storage, read> u: Uniforms;
@group(0) @binding(1) var<storage, read> img_data: array<f32>;

// A quad that covers the full viewport in Normalized Device Coordinates (NDC).
// The corresponding texture coordinates (UVs) for each vertex.
// 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0), // bottom-left
    vec2<f32>(1.0, 0.0), // bottom-right
    vec2<f32>(0.0,  1.0), // top-left
    vec2<f32>(1.0,  1.0)  // top-right
);

const TEX_COORDS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 1.0), // bottom-left
    vec2<f32>(1.0, 1.0), // bottom-right
    vec2<f32>(0.0, 0.0), // top-left
    vec2<f32>(1.0, 0.0)  // top-right
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32
) -> VSOut {
    // Get the texture coordinate for the current vertex.
    let uv = TEX_COORDS[vertex_index];
    // Obtain a position for this vertex in (0 to 1) normalized space.
    let vertex_pos_norm = QUAD[vertex_index];
    let vertex_pos_px = vec2<f32>(
        vertex_pos_norm.x * u.img_size.x,
        vertex_pos_norm.y * u.img_size.y
    );

    // How positioning works for the bitmap layer:
    // If data_unit_mode = Pixels, then the image is positioned in pixel space,
    // with the origin at the bottom left of the layer's bounds (i.e., margins).
    // If data_unit_mode = Data, then the image is positioned in data units,
    // with the origin at (0,0) in data space, and pixels extending positively in x and y directions.

    // The model_matrix can be used to apply additional affine transformations
    // to the physical dimensions of the image (XYZ),
    // such as translation, rotation, and scaling.
    // For example, the model_matrix can be used to account for pixels that are not square,
    // or to adjust the pixel size.
    // (e.g., most bioimaging formats store images with 1 pixel = 1 micrometer,
    // but without a model_matrix specified we assume that 1 pixel = 1 meter).


    // Layer aspect ratio
    // By "layer", we mean the inner plotting area, excluding margins.
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1271C5-L1271C52
    let layer_width_px = u.layer_size.x;
    let layer_height_px = u.layer_size.y;

    let layer_aspect_ratio = layer_width_px / layer_height_px;

    // Get the scale() matrix to handle the aspect ratio mode.
    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        u.aspect_ratio_mode
    );

    // We operate in (0 to 1) space, since it is more intuitive.
    // We therefore need matrices to transform (0, 1) into clip space ("NDC") (-1 to 1)
    let NORM_TO_NDC_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0); // Scale up by 2, THEN translate by -1 (i.e., translating in the scaled-up space)
    // And the inverse, to convert back from NDC (-1 to 1) to normalized (0 to 1) space.
    let NDC_TO_NORM_MAT =  translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0); // Scale down by 0.5, THEN translate by 0.5 (i.e., translating in the scaled-down space)


    // Handle data_unit_mode == "pixels" (we do not care about the camera or aspect_ratio_mode in this case).
    if(u.data_unit_mode == 0u) {
        // Convert point position from pixel space to normalized space (0 to 1)
        let point_pos_norm = vec2<f32>(
            vertex_pos_px.x / layer_width_px,
            vertex_pos_px.y / layer_height_px
        );
        let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

        // TODO: handle the model_matrix

        var out: VSOut;
        out.position = point_pos_ndc;
        out.tex_coord = uv;
        return out;
    }

    // Handle data_unit_mode == "data"

    // TODO: handle the model_matrix

    // Model-view-projection matrix
    // References:
    // - https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1582
    // - https://nalgebra.rs/docs/user_guide/cg_recipes#build-a-mvp-matrix
    let model_view_projection = ASPECT_RATIO_MAT * u.camera_view;

    // TYPICALLY: position = projectionMatrix * viewMatrix * modelMatrix * inputModelSpacePosition
    // Where:
    // - inputPosition - the 4D vertex position (homogeneous coordinate) in model space.
    // - modelMatrix - the 4x4 matrix that transforms input vertices from model space to world space.
    // - viewMatrix - the 4x4 view matrix, which takes as input a point in world space and the result is a point in camera space.
    // - projectionMatrix - the 4x4 projection matrix, which takes as input a point in camera space and the result is a projected point in clip space.

    let point_pos_norm = /*LAYER_NORM_TO_VIEW_NORM_MAT * */ (
        // The camera from dom-2d-camera operates in NDC space.
        // The `dom-2d-camera` library is designed to work in **NDC space (-1 to 1)**, not normalized space (0 to 1).
        // When you zoom in, the scale increases, and when you pan, the translation values are in NDC space.
        // However, after this transformation, we want to be working in (0 to 1) normalized space.

        // The camera operates in NDC space, but your data is in normalized space. We need to:
        // 1. Convert data from (0,1) to NDC (-1,1)
        // 2. Apply camera
        // 3. Convert back to (0,1)
        // 4. Apply aspect ratio and margins
        // 5. Convert final result to NDC for rendering
        // We apply camera AFTER converting to NDC, and DON'T convert back until
        // after all NDC-space operations are done. This keeps translations in the correct space.

        (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT)
        // TODO: support applying a model matrix (arbitrarily passed by the user)
        // before applying the camera (i.e., transforming the data coordinates).
        * vec4(vertex_pos_px, 0.0, 1.0)
    );
    let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);


    var out: VSOut;
    out.position = point_pos_ndc;
    out.tex_coord = uv;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Use image dimensions from uniforms to convert normalized coordinates to pixel coordinates.
    let tex_dims = u.img_size;

    // Calculate integer pixel coordinates from normalized texture coordinates.
    // We need to clamp to avoid reading out of bounds if tex_coord is exactly 1.0.
    let texel_coords = vec2<u32>(
        min(
            floor(in.tex_coord * tex_dims),
            tex_dims - vec2<f32>(1.0, 1.0)
        )
    );

    var final_color = vec3<f32>(0.0, 0.0, 0.0);

    // Loop over num_channels
    for (var channel_index: u32 = 0u; channel_index < u.num_channels; channel_index++) {
        // Compute the flat index into the storage buffer using per-dimension strides.
        // This handles any dimension ordering (e.g., CYX, YXC, XYC, etc.).
        let idx = texel_coords.y * u.y_stride + texel_coords.x * u.x_stride + channel_index * u.c_stride;
        let intensity = img_data[idx];
        let ch_color = u.channels[channel_index].color;
        let ch_window = u.channels[channel_index].window;

        // Apply windowing to adjust contrast limits.
        // The window (min, max) values should be in the same units as the original data values.
        // Reference: https://github.com/hms-dbmi/viv/blob/08a74203b99f54bc62307c741944ed61e33e810c/packages/layers/src/xr-layer/shader-modules/channel-intensity.js#L2
        let windowed = clamp((intensity - ch_window.x) / max(0.0005, (ch_window.y - ch_window.x)), 0.0, 1.0);

        // Additively blend the colors based on their intensity.
        // References:
        // - https://github.com/hms-dbmi/viv/blob/main/packages/extensions/src/color-palette-extension/color-palette-module.js
        // - https://github.com/hms-dbmi/viv/blob/08a74203b99f54bc62307c741944ed61e33e810c/packages/layers/src/xr-layer/xr-layer-fragment.glsl.js#L39
        final_color += ch_color * windowed;
    }

    // Output the blended color.
    return vec4<f32>(final_color, 1.0);
}
