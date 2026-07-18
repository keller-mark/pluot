// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
// Used by the color module below to read per-element color value textures.
{{flat_texel_coord}}

// Computes the final vertex position for a line quad.
// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/layers/src/line-layer/line-layer-vertex.glsl.h#L56
fn extrude_line(
    source_ndc: vec2<f32>,
    target_ndc: vec2<f32>,
    corner: vec2<f32>,
    line_width_ndc: f32,
    viewport_aspect_ratio: f32
) -> vec2<f32> {
    let p0 = source_ndc;
    let p1 = target_ndc;

    // Correct the aspect ratio of the line direction vector
    // so that the normal is perpendicular in screen space.
    var dir = p1 - p0;
    dir.y /= viewport_aspect_ratio;
    dir = normalize(dir);

    // Calculate the normal vector strictly in screen space.
    let normal = vec2<f32>(-dir.y, dir.x);

    // Transform normal back to NDC space for extrusion.
    // The X component needs to be scaled down by aspect ration because
    // NDC X units are wider than NDC Y units (physically).
    // The Y component is kept as 1.0 scaling because line_width_ndc is defined relative to height.
    let extrusion = vec2<f32>(normal.x / viewport_aspect_ratio, normal.y) * line_width_ndc * 0.5;

    // Select the base point (source or target) and apply the extrusion.
    // corner.x is -1 for source, +1 for target.
    // corner.y is -1 or +1 for the side of the line.
    let base_point = mix(p0, p1, (corner.x + 1.0) / 2.0);
    return base_point + corner.y * extrusion;
}

struct LineLayerUniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32, // 0: px units, 1: data coordinate system units
    data_unit_mode_y: u32, // 0: px units, 1: data coordinate system units
    line_width: f32,
    line_width_unit_mode: u32, // 0: px units, 1: data coordinate system units // TODO: use this
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end
    model_matrix: mat4x4<f32>,
    stroke_color_mode: u32, // see ColorMode::shader_mode()
    stroke_color: vec4<f32>, // rgba color used by the UniformRgb mode
    stroke_color_reverse: u32, // 1 = reverse the quantitative colormap
    stroke_color_domain: vec2<f32>, // (min, max) normalization domain for quantitative mode
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) instance_index: u32,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

// These group/binding locations will need to match with the locations used by Model.
@group(0) @binding(0) var<uniform> u: LineLayerUniforms;
// The source/target X/Y coordinate arrays are uploaded as single-channel
// (red-only) 2D textures holding the flat array reshaped into rows: element
// `idx` (the instance index) lives at texel `(idx % width, idx / width)`. The
// data is NOT reordered on the CPU, so the shader recomputes the 2D texel
// coords from the instance index. Each texture's sampled type is injected at
// runtime by the shader-module system (see `crate::shader_modules`) so that
// 8/16/32-bit data lives on the GPU at native width: `f32` for floating-point
// data, `u32` for unsigned, `i32` for signed. Each array is independent and
// may differ in dtype.
@group(0) @binding(1) var source_x_coords: texture_2d<{{source_x_coords_dtype}}>;
@group(0) @binding(2) var source_y_coords: texture_2d<{{source_y_coords_dtype}}>;
@group(0) @binding(3) var target_x_coords: texture_2d<{{target_x_coords_dtype}}>;
@group(0) @binding(4) var target_y_coords: texture_2d<{{target_y_coords_dtype}}>;

// Color module: any per-element color value/palette texture bindings (from
// binding 5 onward) plus `fn get_stroke_color(instance_index: u32) -> vec3<f32>`.
// Assembled per color mode by `crate::color_mode::prepare_stroke_color`.
{{color_module}}


// 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0,  1.0)
);


@vertex
fn vs_main(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32
) -> VSOut {
    // Source and target points of this line. Map the flat instance index into the
    // 2D texture each coordinate array was reshaped into on upload:
    // (idx % width, idx / width). `f32(...)` is a no-op when the injected
    // sampled type is already f32, and widens u32/i32 texels to f32 otherwise.
    let source_x_tex_width = textureDimensions(source_x_coords).x;
    let source_y_tex_width = textureDimensions(source_y_coords).x;
    let target_x_tex_width = textureDimensions(target_x_coords).x;
    let target_y_tex_width = textureDimensions(target_y_coords).x;
    let source_x_val = f32(textureLoad(source_x_coords, vec2<u32>(instance_index % source_x_tex_width, instance_index / source_x_tex_width), 0).x);
    let source_y_val = f32(textureLoad(source_y_coords, vec2<u32>(instance_index % source_y_tex_width, instance_index / source_y_tex_width), 0).x);
    let target_x_val = f32(textureLoad(target_x_coords, vec2<u32>(instance_index % target_x_tex_width, instance_index / target_x_tex_width), 0).x);
    let target_y_val = f32(textureLoad(target_y_coords, vec2<u32>(instance_index % target_y_tex_width, instance_index / target_y_tex_width), 0).x);
    let source_point_pos_orig = u.model_matrix * vec4f(source_x_val, source_y_val, 0.0, 1.0);
    let target_point_pos_orig = u.model_matrix * vec4f(target_x_val, target_y_val, 0.0, 1.0);

    // TODO: adapt the rest of the code to draw lines rather than points.

    let corner = QUAD[vertex_index & 3u]; // vertex_index % 4

    // Layer aspect ratio
    // By "layer", we mean the inner plotting area, excluding margins.
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1271C5-L1271C52
    let layer_width_px = u.layer_size.x;
    let layer_height_px = u.layer_size.y;

    let layer_aspect_ratio = layer_width_px / layer_height_px;

    // Get the scale() matrix to handle the aspect ratio mode.
    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        u.aspect_ratio_mode,
        u.aspect_ratio_alignment_mode
    );

    // We operate in (0 to 1) space, since it is more intuitive.
    // We therefore need matrices to transform (0, 1) into clip space ("NDC") (-1 to 1)
    let NORM_TO_NDC_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0); // Scale up by 2, THEN translate by -1 (i.e., translating in the scaled-up space)
    // And the inverse, to convert back from NDC (-1 to 1) to normalized (0 to 1) space.
    let NDC_TO_NORM_MAT =  translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0); // Scale down by 0.5, THEN translate by 0.5 (i.e., translating in the scaled-down space)


    var result_source_position_px = vec2<f32>(0.0, 0.0);
    var result_target_position_px = vec2<f32>(0.0, 0.0);

    var result_source_position_data = vec2<f32>(0.0, 0.0);
    var result_target_position_data = vec2<f32>(0.0, 0.0);

    // Handle data_unit_mode == "pixels" (we do not care about the camera or aspect_ratio_mode in this case).
    if(u.data_unit_mode_x == 0u || u.data_unit_mode_y == 0u) {
        // Both source and target points are in pixel coordinates.
        // Convert them to normalized (0 to 1) coordinates within the layer.
        let source_point_pos_px = source_point_pos_orig;
        let target_point_pos_px = target_point_pos_orig;

        let source_point_pos_norm = vec2<f32>(
            source_point_pos_px.x / layer_width_px,
            source_point_pos_px.y / layer_height_px
        );
        let target_point_pos_norm = vec2<f32>(
            target_point_pos_px.x / layer_width_px,
            target_point_pos_px.y / layer_height_px
        );

        // Convert to NDC for extrusion calculation
        let source_pos_ndc = (NORM_TO_NDC_MAT * vec4f(source_point_pos_norm.xy, 0.0, 1.0)).xy;
        let target_pos_ndc = (NORM_TO_NDC_MAT * vec4f(target_point_pos_norm.xy, 0.0, 1.0)).xy;

        result_source_position_px = source_pos_ndc;
        result_target_position_px = target_pos_ndc;

        let line_width_ndc = u.line_width / layer_height_px * 2.0;

        if(u.data_unit_mode_x == 0u && u.data_unit_mode_y == 0u) {
            // Extrude the line to form a quad
            let point_pos_ndc = extrude_line(
                result_source_position_px,
                result_target_position_px,
                corner,
                line_width_ndc,
                layer_aspect_ratio
            );

            // The final point position in NDC space.
            let pos = vec4f(
                point_pos_ndc.x,
                point_pos_ndc.y,
                0.0,
                1.0
            );

            var out: VSOut;
            out.position = pos;
            out.instance_index = instance_index;
            return out;
        }
    }

    // Model-view-projection matrix
    // References:
    // - https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1582
    // - https://nalgebra.rs/docs/user_guide/cg_recipes#build-a-mvp-matrix
    let model_view_projection = ASPECT_RATIO_MAT * u.camera_view;

    let transform_mat = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT);

    // Transform source and target points to normalized view space
    let source_pos_norm = transform_mat * source_point_pos_orig;
    let target_pos_norm = transform_mat * target_point_pos_orig;

    // Convert to NDC for extrusion calculation
    let source_pos_ndc = (NORM_TO_NDC_MAT * vec4f(source_pos_norm.xy, 0.0, 1.0)).xy;
    let target_pos_ndc = (NORM_TO_NDC_MAT * vec4f(target_pos_norm.xy, 0.0, 1.0)).xy;

    // TODO: Handle line_width_unit_mode == 1 (data coordinates)
    // TODO: once supporting data unit sizing, apply the model_matrix to the size as needed.
    let line_width_ndc = u.line_width / layer_height_px * 2.0;

    result_source_position_data = source_pos_ndc;
    result_target_position_data = target_pos_ndc;

    if(u.data_unit_mode_x == 0u) {
        // Want to use pixel-based positioning, but only along X direction.
        result_source_position_data.x = result_source_position_px.x;
        result_target_position_data.x = result_target_position_px.x;
    }
    if(u.data_unit_mode_y == 0u) {
        // Want to use pixel-based positioning, but only along Y direction.
        result_source_position_data.y = result_source_position_px.y;
        result_target_position_data.y = result_target_position_px.y;
    }

    // Extrude the line to form a quad
    let point_pos_ndc = extrude_line(
        result_source_position_data,
        result_target_position_data,
        corner,
        line_width_ndc,
        layer_aspect_ratio
    );

    // The final point position in NDC space.
    let pos = vec4f(
        point_pos_ndc.x,
        point_pos_ndc.y,
        0.0,
        1.0
    );

    var out: VSOut;
    out.position = pos;
    out.instance_index = instance_index;
    return out;
}


@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) @interpolate(flat) instance_index: u32,
) -> FSOut {

    // The color module's get_stroke_color resolves the per-instance color for the
    // active color mode (static, instanced RGB, categorical or quantitative).
    let out_color = get_stroke_color(instance_index);

    var out: FSOut;
    out.color = vec4<f32>(out_color, 1.0);
    return out;
}
