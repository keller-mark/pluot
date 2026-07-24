// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
// Used by the color module below to read per-element color value textures.
{{flat_texel_coord}}

struct PointLayerUniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32, // 0: px units, 1: data coordinate system units, 2: normalized (0-1) units
    data_unit_mode_y: u32, // 0: px units, 1: data coordinate system units, 2: normalized (0-1) units
    point_radius: f32,
    point_radius_unit_mode_x: u32, // 0: px units, 1: data coordinate system units, 2: normalized (0-1) units
    point_radius_unit_mode_y: u32, // 0: px units, 1: data coordinate system units, 2: normalized (0-1) units
    point_shape_mode: u32, // 0: square; 1: circle
    fill_opacity: f32,
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end
    model_matrix: mat4x4<f32>,
    fill_color_mode: u32, // see ColorMode::shader_mode()
    fill_color: vec4<f32>, // rgba color used by the UniformRgb mode
    fill_color_reverse: u32, // 1 = reverse the quantitative colormap
    fill_color_domain: vec2<f32>, // (min, max) normalization domain for quantitative mode
    stroke_width: f32,
    stroke_width_unit_mode: u32, // 0: px units, 1: data coordinate system units, 2: normalized (0-1) units
    stroke_color_mode: u32, // see ColorMode::shader_mode()
    stroke_color: vec4<f32>, // rgba color used by the UniformRgb mode
    stroke_color_reverse: u32, // 1 = reverse the quantitative colormap
    stroke_color_domain: vec2<f32>, // (min, max) normalization domain for quantitative mode
    stroke_opacity: f32, // stroke opacity used by the UniformOpacity mode
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
    @location(2) @interpolate(flat) point_radius_px: f32,
    // Per-instance stroke width in pixels, resolved from the stroke-width module.
    @location(3) @interpolate(flat) stroke_width_px: f32,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: PointLayerUniforms;
// The X/Y coordinate arrays are uploaded as single-channel (red-only) 2D
// textures holding the flat array reshaped into rows: element `idx` (the
// instance index) lives at texel `(idx % width, idx / width)`. The data is NOT
// reordered on the CPU, so the shader recomputes the 2D texel coords from the
// instance index. Each texture's sampled type is injected at runtime by the
// shader-module system (see `crate::shader_modules`) so that 8/16/32-bit data
// lives on the GPU at native width: `f32` for floating-point data, `u32` for
// unsigned, `i32` for signed. X and Y are independent and may differ in dtype.
@group(0) @binding(1) var x_coords: texture_2d<{{x_coords_dtype}}>;
@group(0) @binding(2) var y_coords: texture_2d<{{y_coords_dtype}}>;

// Fill-color module: any per-element color value/palette texture bindings (from
// binding 3 onward) plus `fn get_fill_color(instance_index: u32) -> vec3<f32>`.
// Assembled per color mode by `crate::color_mode::prepare_color_mode`.
{{fill_color_module}}

// Stroke-color module: the stroke counterpart, defining
// `fn get_stroke_color(instance_index: u32) -> vec3<f32>`. Assembled by
// `crate::color_mode::prepare_stroke_color`.
{{stroke_color_module}}

// Size module: an optional per-element radius value texture (instanced mode)
// plus `fn get_point_radius(instance_index: u32) -> f32`. Assembled per size
// mode by `crate::scalar_mode::prepare_size_mode`.
{{size_module}}

// Stroke-width module: an optional per-element width value texture (instanced
// mode, read in the vertex stage) plus `fn get_stroke_width(instance_index: u32)
// -> f32`. Assembled by `crate::scalar_mode::prepare_stroke_width_mode`.
{{stroke_width_module}}

// Fill-opacity module: an optional per-element opacity value texture (instanced
// mode) plus `fn get_fill_opacity(instance_index: u32) -> f32`. Assembled by
// `crate::scalar_mode::prepare_fill_opacity_mode`.
{{fill_opacity_module}}

// Stroke-opacity module: an optional per-element opacity value texture
// (instanced mode) plus `fn get_stroke_opacity(instance_index: u32) -> f32`.
// Assembled by `crate::scalar_mode::prepare_stroke_opacity_mode`.
{{stroke_opacity_module}}


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
    // Center of this point in data space. Map the flat instance index into the
    // 2D texture the coordinates were reshaped into on upload:
    // (idx % width, idx / width). `f32(...)` is a no-op when the injected
    // sampled type is already f32, and widens u32/i32 texels to f32 otherwise.
    let x_tex_width = textureDimensions(x_coords).x;
    let y_tex_width = textureDimensions(y_coords).x;
    let x_val = f32(textureLoad(x_coords, flat_texel_coord(instance_index, x_tex_width), 0).x);
    let y_val = f32(textureLoad(y_coords, flat_texel_coord(instance_index, y_tex_width), 0).x);
    let point_pos_orig = u.model_matrix * vec4f(x_val, y_val, 0.0, 1.0);

    // Per-instance radius (uniform or instanced, depending on the injected size
    // module). Resolved once here and used for all radius computations below.
    let point_radius = get_point_radius(instance_index);

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

    // Model-view-projection matrix
    // References:
    // - https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1582
    // - https://nalgebra.rs/docs/user_guide/cg_recipes#build-a-mvp-matrix
    let model_view_projection = ASPECT_RATIO_MAT * u.camera_view;

    // --- Point radius in NDC space ---
    //
    // Pixel-mode radius (point_radius_unit_mode == 0):
    //   point_radius is in screen pixels; convert directly to NDC offsets.
    let point_radius_ndc_px = vec2f(
        point_radius / layer_width_px * 2.0,
        point_radius / layer_height_px * 2.0
    );

    // Data-coordinate radius (point_radius_unit_mode == 1):
    //   point_radius is in data coordinate system units. Transform it through the same
    //   pipeline as positions, but with w=0 so translations cancel out (it is a delta/size,
    //   not a position). This mirrors get_point_size() in positioning.rs.
    let radius_orig_data = u.model_matrix * vec4f(point_radius, point_radius, 0.0, 0.0);
    let radius_norm_data = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT) * radius_orig_data;
    let point_radius_ndc_data = abs(radius_norm_data.xy) * 2.0;
    // Effective pixel radius for the circle SDF: average of x and y screen extents.
    let point_radius_px_data = (abs(radius_norm_data.x) * layer_width_px + abs(radius_norm_data.y) * layer_height_px) * 0.5;

    // Normalized-mode radius (point_radius_unit_mode == 2):
    //   point_radius is a fraction (0 to 1) of the layer height, independent of the
    //   camera. This mirrors the height-relative convention used for stroke width
    //   in LineLayer/RectLayer.
    let point_radius_px_normalized = point_radius * layer_height_px;
    let point_radius_ndc_normalized = vec2f(
        point_radius_px_normalized / layer_width_px * 2.0,
        point_radius_px_normalized / layer_height_px * 2.0
    );

    // --- Stroke width in pixels ---
    // The stroke is drawn inward from the point edge, so it does not expand the
    // quad; we only need its pixel width for the fragment stage.
    //
    // Pixel mode (stroke_width_unit_mode == 0): stroke_width is already in pixels.
    //
    // Data-coordinate mode (== 1): transform it through the same pipeline as the
    // radius, with w = 0 so translations cancel (it is a delta/size, not a
    // position), and take the average of the x/y screen extents.
    let stroke_width = get_stroke_width(instance_index);
    var stroke_width_px: f32 = stroke_width;
    if (u.stroke_width_unit_mode == 1u) {
        let sw_orig_data = u.model_matrix * vec4f(stroke_width, stroke_width, 0.0, 0.0);
        let sw_norm_data = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT) * sw_orig_data;
        stroke_width_px = (abs(sw_norm_data.x) * layer_width_px + abs(sw_norm_data.y) * layer_height_px) * 0.5;
    } else if (u.stroke_width_unit_mode == 2u) {
        // Normalized mode: stroke_width is a fraction (0 to 1) of the layer
        // height, independent of the camera.
        stroke_width_px = stroke_width * layer_height_px;
    }

    // Select per-axis NDC radius and the scalar pixel radius passed to the fragment shader.
    var point_radius_ndc_x = point_radius_ndc_px.x;
    var point_radius_ndc_y = point_radius_ndc_px.y;
    var point_radius_px: f32 = point_radius;

    if (u.point_radius_unit_mode_x == 1u) {
        point_radius_ndc_x = point_radius_ndc_data.x;
        point_radius_px = point_radius_px_data;
    } else if (u.point_radius_unit_mode_x == 2u) {
        point_radius_ndc_x = point_radius_ndc_normalized.x;
        point_radius_px = point_radius_px_normalized;
    }
    if (u.point_radius_unit_mode_y == 1u) {
        point_radius_ndc_y = point_radius_ndc_data.y;
        point_radius_px = point_radius_px_data;
    } else if (u.point_radius_unit_mode_y == 2u) {
        point_radius_ndc_y = point_radius_ndc_normalized.y;
        point_radius_px = point_radius_px_normalized;
    }

    var result_position_px = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var result_position_data = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    // Handle data_unit_mode == "pixels" or "normalized" (we do not care about the
    // camera or aspect_ratio_mode in either case; they are both camera-independent).
    if(u.data_unit_mode_x != 1u || u.data_unit_mode_y != 1u) {
        // Pixel-mode points are in pixel coordinates and are converted to normalized
        // (0 to 1) coordinates within the layer by dividing by the layer size.
        // Normalized-mode points are already in (0 to 1) coordinates, so are used as-is.
        let point_pos_norm = vec2<f32>(
            select(point_pos_orig.x / layer_width_px, point_pos_orig.x, u.data_unit_mode_x == 2u),
            select(point_pos_orig.y / layer_height_px, point_pos_orig.y, u.data_unit_mode_y == 2u)
        );
        let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

        // The final point position in NDC space.
        result_position_px = vec4f(
            point_pos_ndc.x + (corner.x * point_radius_ndc_x),
            point_pos_ndc.y + (corner.y * point_radius_ndc_y),
            0.0,
            1.0
        );

        if(u.data_unit_mode_x != 1u && u.data_unit_mode_y != 1u) {
            var out: VSOut;
            out.position = result_position_px;
            out.corner = corner;
            out.instance_index = instance_index;
            out.point_radius_px = point_radius_px;
            out.stroke_width_px = stroke_width_px;
            return out;
        }
    }


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
        // Support applying a model matrix (arbitrarily passed by the user)
        // before applying the camera (i.e., transforming the data coordinates).
        * point_pos_orig
    );
    let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

    // The final point position in NDC space.
    result_position_data = vec4f(
        point_pos_ndc.x + (corner.x * point_radius_ndc_x),
        point_pos_ndc.y + (corner.y * point_radius_ndc_y),
        0.0,
        1.0
    );

    if(u.data_unit_mode_x != 1u) {
        // Want to use pixel/normalized-based positioning, but only along X direction.
        result_position_data.x = result_position_px.x;
    }
    if(u.data_unit_mode_y != 1u) {
        // Want to use pixel/normalized-based positioning, but only along Y direction.
        result_position_data.y = result_position_px.y;
    }

    var out: VSOut;
    out.position = result_position_data;
    out.corner = corner;
    out.instance_index = instance_index;
    out.point_radius_px = point_radius_px;
    out.stroke_width_px = stroke_width_px;
    return out;
}


fn linearstep(edge0: f32, edge1: f32, x: f32) -> f32 {
  return clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
    @location(2) @interpolate(flat) point_radius_px: f32,
    @location(3) @interpolate(flat) stroke_width_px: f32,
) -> FSOut {

    // The color / opacity modules resolve the per-instance value for the active
    // mode (static, instanced RGB, categorical or quantitative for color; static
    // or instanced for opacity). Interior fragments use the fill; fragments within
    // the stroke band (the outermost `stroke_width_px` of the point) use the
    // stroke. The stroke is drawn inward, so the point's outer bound stays at
    // `point_radius_px` regardless of stroke width.
    let fill_color = get_fill_color(instance_index);
    let fill_opacity = get_fill_opacity(instance_index);
    let stroke_color = get_stroke_color(instance_index);
    let stroke_opacity = get_stroke_opacity(instance_index);

    // Radius of the inner boundary between fill and stroke.
    let inner_radius_px = point_radius_px - stroke_width_px;

    var out_color: vec3<f32>;
    var alpha: f32;

    // Handling of circle point shape mode
    // TODO: see https://github.com/visgl/deck.gl/blob/6149b4c4ca5e33397d697c21d6729cb2cf8e4c89/modules/layers/src/scatterplot-layer/scatterplot-layer.wgsl.ts#L157
    if(u.point_shape_mode == 1u) {
        // Signed-distance anti-aliasing: linear 1-pixel coverage fade centered on
        // the circle edge (independent of opacity).
        let dist_px = length(corner) * point_radius_px;
        let coverage = clamp(point_radius_px - dist_px + 0.475, 0.0, 1.0);
        if (coverage < 0.001) {
            discard;
        }
        // Interior (fill) vs. the outer stroke band.
        if (stroke_width_px > 0.0 && dist_px > inner_radius_px) {
            out_color = stroke_color;
            alpha = coverage * stroke_opacity;
        } else {
            out_color = fill_color;
            alpha = coverage * fill_opacity;
        }
    } else {
        // Square shape: the stroke band is the outermost `stroke_width_px` of the
        // square along either axis. corner is in [-1, 1]; scale to pixels.
        let px = abs(corner.x) * point_radius_px;
        let py = abs(corner.y) * point_radius_px;
        if (stroke_width_px > 0.0 && (px > inner_radius_px || py > inner_radius_px)) {
            out_color = stroke_color;
            alpha = stroke_opacity;
        } else {
            out_color = fill_color;
            alpha = fill_opacity;
        }
    }

    var out: FSOut;
    out.color = vec4<f32>(out_color, alpha);
    return out;
}
