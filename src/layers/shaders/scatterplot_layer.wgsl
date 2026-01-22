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

struct ScatterplotLayerUniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode: u32, // 0: pixel units, 1: data units
    point_radius: f32,
    point_radius_unit_mode: u32, // 0: px units, 1: data coordinate system units
    point_shape_mode: u32, // 0: square; 1: circle
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end
    color: vec4<f32>,     // rgba color for points
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
    @location(3) valid_bounds: vec4<f32>, // valid_min.xy, valid_max.xy in NDC
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: ScatterplotLayerUniforms;
@group(0) @binding(1) var<storage, read> x_coords: array<f32>;
@group(0) @binding(2) var<storage, read> y_coords: array<f32>;
@group(0) @binding(3) var<storage, read> labels_coords: array<i32>;


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
    // Center of this point in data space
    let point_pos_orig = vec2<f32>(x_coords[instance_index], y_coords[instance_index]);

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
            point_pos_orig.x / layer_width_px,
            point_pos_orig.y / layer_height_px
        );
        let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

        // Compute the vertex position by accounting for point position and point size.
        let point_radius_norm = vec4f(
            u.point_radius / layer_width_px,
            u.point_radius / layer_height_px,
            0.0,
            1.0
        );
        let point_radius_ndc = vec4f(point_radius_norm.xy * 2.0, 0.0, 1.0);

        // The final point position in NDC space.
        let pos = vec4f(
            point_pos_ndc.x + (corner.x * point_radius_ndc.x),
            point_pos_ndc.y + (corner.y * point_radius_ndc.y),
            0.0,
            1.0
        );

        var out: VSOut;
        out.position = pos;
        out.color = u.color;
        // Pass quad position in [0, 1] range for fragment shader.
        out.quad_pos = (corner + 1.0) * 0.5;
        out.instance_index = instance_index;
        // Pass valid bounds to fragment shader for per-fragment clipping
        out.valid_bounds = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return out;
    }


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
        * vec4(point_pos_orig, 0.0, 1.0)
    );
    let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

    // Compute the vertex position by accounting for point position and point size.
    // TODO: support a "point radius mode" to allow setting the point radius in data coordinate system units.
    let point_radius_norm = vec4f(
        u.point_radius / layer_width_px,
        u.point_radius / layer_height_px,
        0.0,
        1.0
    );
    let point_radius_ndc = vec4f(point_radius_norm.xy * 2.0, 0.0, 1.0);

    // The final point position in NDC space.
    let pos = vec4f(
        point_pos_ndc.x + (corner.x * point_radius_ndc.x),
        point_pos_ndc.y + (corner.y * point_radius_ndc.y),
        0.0,
        1.0
    );

    var out: VSOut;
    out.position = pos;
    out.color = u.color;
    // Pass quad position in [0, 1] range for fragment shader.
    out.quad_pos = (corner + 1.0) * 0.5;
    out.instance_index = instance_index;
    // Pass valid bounds to fragment shader for per-fragment clipping
    out.valid_bounds = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    return out;
}


fn get_categorical_color(index: i32) -> vec4<f32> {
    // Simple categorical colormap (Tableau 10)
    const colors: array<vec4<f32>, 10> = array<vec4<f32>, 10>(
        vec4<f32>(31.0, 119.0, 180.0, 255.0) / 255.0,
        vec4<f32>(255.0, 127.0, 14.0, 255.0) / 255.0,
        vec4<f32>(44.0, 160.0, 44.0, 255.0) / 255.0,
        vec4<f32>(214.0, 39.0, 40.0, 255.0) / 255.0,
        vec4<f32>(148.0, 103.0, 189.0, 255.0) / 255.0,
        vec4<f32>(227.0, 119.0, 194.0, 255.0) / 255.0,
        vec4<f32>(127.0, 127.0, 127.0, 255.0) / 255.0,
        vec4<f32>(188.0, 189.0, 34.0, 255.0) / 255.0,
        vec4<f32>(23.0, 190.0, 207.0, 255.0) / 255.0,
        vec4<f32>(219.0, 219.0, 219.0, 255.0) / 255.0
    );
    return colors[index % 10];
}

fn linearstep(edge0: f32, edge1: f32, x: f32) -> f32 {
  return clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) color_in: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
    @location(3) valid_bounds: vec4<f32>,
) -> FSOut {
    /*
    // Convert fragment coordinate from screen space back to NDC
    let screen_pos = frag_coord.xy;
    let ndc_pos = vec2<f32>(
        (screen_pos.x / u.viewport_size.x) * 2.0 - 1.0,        // X unchanged
        1.0 - (screen_pos.y / u.viewport_size.y) * 2.0         // Y flipped
    );

    // Extract valid bounds
    let valid_min = valid_bounds.xy;
    let valid_max = valid_bounds.zw;

    // Check if this fragment is outside the valid region
    if (ndc_pos.x < valid_min.x || ndc_pos.x > valid_max.x ||
        ndc_pos.y < valid_min.y || ndc_pos.y > valid_max.y) {
        discard;
    }


    // Anti-aliased circle using linearstep, based on https://github.com/flekschas/regl-scatterplot/blob/main/src/point.fs
    let radius_px = u.point_size_px / 2.0;
    let antiAliasing = 0.5; // Reference: https://github.com/flekschas/regl-scatterplot/blob/90f0c951233b20bebd4fd1cb15ce1c4128ce9edf/src/constants.js#L175
    let c = quad_pos * 2.0 - 1.0;
    let sdf = length(c) * radius_px;
    let alpha = linearstep(radius_px + antiAliasing, radius_px - antiAliasing, sdf);

    if (alpha == 0.0) {
        discard;
    }
*/
    let category_color = get_categorical_color(labels_coords[instance_index]);

    var out: FSOut;
    // Output premultiplied alpha to work with PREMULTIPLIED_ALPHA blending
    out.color = vec4<f32>(category_color.rgb, 1.0);
    return out;
}
