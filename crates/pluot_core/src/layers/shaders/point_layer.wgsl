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

fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: u32, aspect_ratio_alignment_mode: u32) -> mat4x4<f32> {
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

    // To handle aspect_ratio_alignment_mode, we compute the required translation.
    // After scale(sx, sy), the data axis spans [-sx, +sx] in NDC.
    // Center (default): no translation needed.
    // Start: We shift so the start edge aligns to -1. So, tx = sx - 1
    // End: We shift so the end edge aligns to +1.     So, tx = 1 - sx
    // When the scaling is 1.0, both formulas yield 0.
    var x_translation_for_aspect_ratio_alignment_mode = 0.0;
    var y_translation_for_aspect_ratio_alignment_mode = 0.0;
    if (aspect_ratio_alignment_mode == 1u) {
        // start
        x_translation_for_aspect_ratio_alignment_mode = x_scale_for_aspect_ratio_mode - 1.0;
        y_translation_for_aspect_ratio_alignment_mode = y_scale_for_aspect_ratio_mode - 1.0;
    } else if (aspect_ratio_alignment_mode == 2u) {
        // end
        x_translation_for_aspect_ratio_alignment_mode = 1.0 - x_scale_for_aspect_ratio_mode;
        y_translation_for_aspect_ratio_alignment_mode = 1.0 - y_scale_for_aspect_ratio_mode;
    }

    return translate(
        x_translation_for_aspect_ratio_alignment_mode,
        y_translation_for_aspect_ratio_alignment_mode,
        0.0
    ) * scale(
        x_scale_for_aspect_ratio_mode,
        y_scale_for_aspect_ratio_mode,
        1.0
    );
}

struct PointLayerUniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32, // 0: pixel units, 1: data units
    data_unit_mode_y: u32, // 0: pixel units, 1: data units
    point_radius: f32,
    point_radius_unit_mode_x: u32, // 0: px units, 1: data coordinate system units
    point_radius_unit_mode_y: u32, // 0: px units, 1: data coordinate system units
    point_shape_mode: u32, // 0: square; 1: circle
    point_opacity: f32,
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end
    model_matrix: mat4x4<f32>,
    fill_color_mode: u32, // 0: static color for all points; 1: categorical // TODO: expand this, remove hard-coded categorical logic
    fill_color: vec4<f32>, // rgba color
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
    @location(2) @interpolate(flat) point_radius_px: f32,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: PointLayerUniforms;
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
    let point_pos_orig = u.model_matrix * vec4f(x_coords[instance_index], y_coords[instance_index], 0.0, 1.0);

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
        u.point_radius / layer_width_px * 2.0,
        u.point_radius / layer_height_px * 2.0
    );

    // Data-coordinate radius (point_radius_unit_mode == 1):
    //   point_radius is in data coordinate system units. Transform it through the same
    //   pipeline as positions, but with w=0 so translations cancel out (it is a delta/size,
    //   not a position). This mirrors get_point_size() in positioning.rs.
    let radius_orig_data = u.model_matrix * vec4f(u.point_radius, u.point_radius, 0.0, 0.0);
    let radius_norm_data = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT) * radius_orig_data;
    let point_radius_ndc_data = abs(radius_norm_data.xy) * 2.0;
    // Effective pixel radius for the circle SDF: average of x and y screen extents.
    let point_radius_px_data = (abs(radius_norm_data.x) * layer_width_px + abs(radius_norm_data.y) * layer_height_px) * 0.5;

    // Select per-axis NDC radius and the scalar pixel radius passed to the fragment shader.
    var point_radius_ndc_x = point_radius_ndc_px.x;
    var point_radius_ndc_y = point_radius_ndc_px.y;
    var point_radius_px: f32 = u.point_radius;

    if (u.point_radius_unit_mode_x == 1u) {
        point_radius_ndc_x = point_radius_ndc_data.x;
        point_radius_px = point_radius_px_data;
    }
    if (u.point_radius_unit_mode_y == 1u) {
        point_radius_ndc_y = point_radius_ndc_data.y;
        point_radius_px = point_radius_px_data;
    }

    var result_position_px = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var result_position_data = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    // Handle data_unit_mode == "pixels" (we do not care about the camera or aspect_ratio_mode in this case).
    if(u.data_unit_mode_x == 0u || u.data_unit_mode_y == 0u) {
        // Convert point position from pixel space to normalized space (0 to 1)
        let point_pos_norm = vec2<f32>(
            point_pos_orig.x / layer_width_px,
            point_pos_orig.y / layer_height_px
        );
        let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

        // The final point position in NDC space.
        result_position_px = vec4f(
            point_pos_ndc.x + (corner.x * point_radius_ndc_x),
            point_pos_ndc.y + (corner.y * point_radius_ndc_y),
            0.0,
            1.0
        );

        if(u.data_unit_mode_x == 0u && u.data_unit_mode_y == 0u) {
            var out: VSOut;
            out.position = result_position_px;
            out.corner = corner;
            out.instance_index = instance_index;
            out.point_radius_px = point_radius_px;
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

    if(u.data_unit_mode_x == 0u) {
        // Want to use pixel-based positioning, but only along X direction.
        result_position_data.x = result_position_px.x;
    }
    if(u.data_unit_mode_y == 0u) {
        // Want to use pixel-based positioning, but only along Y direction.
        result_position_data.y = result_position_px.y;
    }

    var out: VSOut;
    out.position = result_position_data;
    out.corner = corner;
    out.instance_index = instance_index;
    out.point_radius_px = point_radius_px;
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
    @location(0) corner: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
    @location(2) @interpolate(flat) point_radius_px: f32,
) -> FSOut {

    // Handling of circle point shape mode
    // TODO: see https://github.com/visgl/deck.gl/blob/6149b4c4ca5e33397d697c21d6729cb2cf8e4c89/modules/layers/src/scatterplot-layer/scatterplot-layer.wgsl.ts#L157
    var alpha = u.point_opacity;
    if(u.point_shape_mode == 1u) {
        // Signed-distance anti-aliasing: linear 1-pixel fade centered on the circle edge.
        let dist_px = length(corner) * point_radius_px;
        alpha = clamp(point_radius_px - dist_px + 0.475, 0.0, u.point_opacity);
        if (alpha < 0.001) {
            discard;
        }
    }


    let category_color = get_categorical_color(labels_coords[instance_index]);

    var out: FSOut;
    out.color = vec4<f32>(category_color.rgb, alpha);
    return out;
}
