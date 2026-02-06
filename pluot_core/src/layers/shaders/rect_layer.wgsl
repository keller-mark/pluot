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


struct RectLayerUniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode: u32, // 0: px units, 1: data coordinate system units
    stroke_width: f32,
    stroke_width_unit_mode: u32, // 0: px units, 1: data coordinate system units
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end
    color: vec4<f32>,     // rgba color for points
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

// These group/binding locations will need to match with the locations used by Model.
@group(0) @binding(0) var<uniform> u: RectLayerUniforms;
@group(0) @binding(1) var<storage, read> position_x0_coords: array<f32>;
@group(0) @binding(2) var<storage, read> position_y0_coords: array<f32>;
@group(0) @binding(3) var<storage, read> position_x1_coords: array<f32>;
@group(0) @binding(4) var<storage, read> position_y1_coords: array<f32>;
@group(0) @binding(5) var<storage, read> labels_coords: array<i32>;


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
    // Corner points of this rect
    let source_point_pos_orig = vec2<f32>(position_x0_coords[instance_index], position_y0_coords[instance_index]);
    let target_point_pos_orig = vec2<f32>(position_x1_coords[instance_index], position_y1_coords[instance_index]);

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
        u.aspect_ratio_mode
    );

    // We operate in (0 to 1) space, since it is more intuitive.
    // We therefore need matrices to transform (0, 1) into clip space ("NDC") (-1 to 1)
    let NORM_TO_NDC_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0); // Scale up by 2, THEN translate by -1 (i.e., translating in the scaled-up space)
    // And the inverse, to convert back from NDC (-1 to 1) to normalized (0 to 1) space.
    let NDC_TO_NORM_MAT =  translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0); // Scale down by 0.5, THEN translate by 0.5 (i.e., translating in the scaled-down space)


    // Handle data_unit_mode == "pixels" (we do not care about the camera or aspect_ratio_mode in this case).
    if(u.data_unit_mode == 0u) {
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

        // Compute the center point in normalized coordinates, to use as the origin for rotation and scaling.
        let center_point_pos_norm = (source_point_pos_norm + target_point_pos_norm) / 2.0;

        let half_rect_width_norm = (target_point_pos_norm.x - source_point_pos_norm.x) / 2.0;
        let half_rect_height_norm = (target_point_pos_norm.y - source_point_pos_norm.y) / 2.0;

        let point_pos_norm = vec2f(
            center_point_pos_norm.x + half_rect_width_norm * corner.x,
            center_point_pos_norm.y + half_rect_height_norm * corner.y,
        );

        // TODO: handle rotation.
        let point_pos_ndc = (NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0)).xy;

        // TODO: handle stroke width, and both unit modes for it.
        let stroke_width_ndc = u.stroke_width / layer_height_px * 2.0;

        

        // The final point position in NDC space.
        let pos = vec4f(
            point_pos_ndc.x,
            point_pos_ndc.y,
            0.0,
            1.0
        );

        var out: VSOut;
        out.position = pos;
        out.color = u.color;
        out.quad_pos = (corner + 1.0) * 0.5;
        out.instance_index = instance_index;
        return out;
    }

    // Model-view-projection matrix
    // References:
    // - https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1582
    // - https://nalgebra.rs/docs/user_guide/cg_recipes#build-a-mvp-matrix
    let model_view_projection = ASPECT_RATIO_MAT * u.camera_view;

    let transform_mat = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT);

    // Transform source and target points to normalized view space
    let source_pos_norm = transform_mat * vec4(source_point_pos_orig, 0.0, 1.0);
    let target_pos_norm = transform_mat * vec4(target_point_pos_orig, 0.0, 1.0);

    // Compute the center point in normalized coordinates, to use as the origin for rotation and scaling.
    let center_point_pos_norm = (source_pos_norm + target_pos_norm) / 2.0;

    let half_rect_width_norm = (target_pos_norm.x - source_pos_norm.x) / 2.0;
    let half_rect_height_norm = (target_pos_norm.y - source_pos_norm.y) / 2.0;

    let point_pos_norm = vec2f(
        center_point_pos_norm.x + half_rect_width_norm * corner.x,
        center_point_pos_norm.y + half_rect_height_norm * corner.y,
    );

    // TODO: handle rotation.
    let point_pos_ndc = (NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0)).xy;

    // TODO: Handle stroke_width_unit_mode == 1 (data coordinates)
    let stroke_width_ndc = u.stroke_width / layer_height_px * 2.0;

    // The final point position in NDC space.
    let pos = vec4f(
        point_pos_ndc.x,
        point_pos_ndc.y,
        0.0,
        1.0
    );

    var out: VSOut;
    out.position = pos;
    out.color = u.color;
    out.quad_pos = (corner + 1.0) * 0.5;
    out.instance_index = instance_index;
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


@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) color_in: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
) -> FSOut {

    let category_color = get_categorical_color(labels_coords[instance_index]);

    var out: FSOut;
    // Output premultiplied alpha to work with PREMULTIPLIED_ALPHA blending
    out.color = vec4<f32>(category_color.rgb, 1.0);
    return out;
}
