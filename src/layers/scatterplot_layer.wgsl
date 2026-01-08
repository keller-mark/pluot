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

// Default camera view matrix (identity).
const CAMERA_VIEW_IDENTITY: mat4x4<f32> = mat4x4<f32>(
  vec4<f32>(1.0, 0.0, 0.0, 0.0),
  vec4<f32>(0.0, 1.0, 0.0, 0.0),
  vec4<f32>(0.0, 0.0, 1.0, 0.0),
  vec4<f32>(0.0, 0.0, 0.0, 1.0)
);

struct Uniforms {
    viewport_size: vec2<f32>, // (width, height) in pixels
    plot_margin: vec4<f32>, // (top | right | bottom | left) in pixels
    camera_view: mat4x4<f32>,
    point_radius: f32,
    point_radius_units: u32, // 0: px units, 1: data coordinate system units
    color: vec4<f32>,     // rgba color for points
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
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

// These group/binding locations will need to match with the locations used by Model.
@group(0) @binding(0) var<uniform> u: Uniforms;
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

    // TODO: Display the 0 to 1 square when camera_view is identity.

    // TODO: Handle multiple aspect ratio modes.

    // View aspect ratio
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1271C5-L1271C52
    let view_width_px = u.viewport_size.x;
    let view_height_px = u.viewport_size.y;
    // let view_aspect_ratio = view_width_px / view_height_px; // We don't care about this; We only care about the layer aspect ratio.

    // Layer aspect ratio
    // By "layer", we mean the inner plotting area, excluding margins.
    let margin_top_px = u.plot_margin.x;
    let margin_right_px = u.plot_margin.y;
    let margin_bottom_px = u.plot_margin.z;
    let margin_left_px = u.plot_margin.w;

    let layer_width_px = view_width_px - (margin_left_px + margin_right_px);
    let layer_height_px = view_height_px - (margin_top_px + margin_bottom_px);
    let layer_aspect_ratio = layer_width_px / layer_height_px;

    // Determine the x and y extents to use,
    // based on the aspect ratio mode and layer aspect ratio.
    // We only need to handle the aspect ratio mode when the layer_aspect_ratio is not 1.
    var x_extent_for_aspect_ratio_mode = 1.0;
    var y_extent_for_aspect_ratio_mode = 1.0;
    if (u.aspect_ratio_mode == 1u) {
        // fit/contain
        if (layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show more than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_extent_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show exactly (0, 1) in x direction. Show more than (0, 1) in y direction.
            y_extent_for_aspect_ratio_mode = layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    } else if (u.aspect_ratio_mode == 2u) {
        // fill/cover
        if(layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show exactly (0, 1) in x direction. Show less than (0, 1) in y direction.
            y_extent_for_aspect_ratio_mode = layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show less than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_extent_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    }

    // TODO: is this correct?
    let ASPECT_RATIO_MAT = scale(
        x_extent_for_aspect_ratio_mode, // should this be inverted? 1 / x_extent_for_aspect_ratio_mode?
        y_extent_for_aspect_ratio_mode, // should this be inverted? 1 / y_extent_for_aspect_ratio_mode?
        1.0
    );


    // Model-view-projection matrix
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1582
    /*let projection: mat4x4<f32> = mat4x4<f32>(
        vec4<f32>(1.0 / view_aspect_ratio, 0.0, 0.0, 0.0), // Column 0
        vec4<f32>(0.0, 1.0, 0.0, 0.0), // Column 1
        vec4<f32>(0.0, 0.0, 1.0, 0.0), // Column 2
        vec4<f32>(0.0, 0.0, 0.0, 1.0), // Column 3
    );*/
    //let model_view_projection = projection * u.camera_view;


    let corner = QUAD[vertex_index & 3u]; // vertex_index % 4

    // TODO: handle point size in data coordinate system units.
    let point_size_ndc = vec2<f32>(
        u.point_radius / view_width_px,
        u.point_radius / view_height_px
    );

    // Convert margins from pixel to (0 to 1) normalized units.
    let margin_top_norm = margin_top_px / view_height_px;
    let margin_right_norm = margin_right_px / view_width_px;
    let margin_bottom_norm = margin_bottom_px / view_height_px;
    let margin_left_norm = margin_left_px / view_width_px;

    // Transformation matrix so that points are drawn within the plot area.
    let MARGIN_MAT = translate(
        margin_left_norm, // left
        margin_bottom_norm, // bottom
        0.0
    ) * scale(
        1.0 - (margin_left_norm + margin_right_norm),
        1.0 - (margin_top_norm + margin_bottom_norm),
        1.0
    ); // Scale down by (1 - total_margin), THEN translate the scaled stuff by left/top margins.
    // We operate in (0 to 1) space, since we apply MARGIN_MAT after MODEL_MAT.

    // POSITION = PROJECTION * MODEL * ORIG_POSITION

    // Transform (0, 1) into clip space ("NDC") (-1 to 1)
    // This enables us to work in (0 to 1) space afterwards, which is more intuitive for me at the moment.
    let NORM_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0); // Scale up by 2, THEN translate by -1.

    // TODO: use real camera_view. using identity only for testing.
    //let point_pos_to_ndc = CAMERA_VIEW_IDENTITY * MODEL_MAT * vec4(point_pos_orig, 0.0, 1.0);
    //
    // TYPICALLY: position = projectionMatrix * viewMatrix * modelMatrix * inputModelSpacePosition
    // Where:
    // - inputPosition - the 4D vertex position (homogeneous coordinate) in model space.
    // - modelMatrix - the 4x4 matrix that transforms input vertices from model space to world space.
    // - viewMatrix - the 4x4 view matrix, which takes as input a point in world space and the result is a point in camera space.
    // - projectionMatrix - the 4x4 projection matrix, which takes as input a point in camera space and the result is a projected point in clip space.

    // TODO: is the ordering of this correct?
    let point_pos_to_ndc = NORM_MAT * MARGIN_MAT * ASPECT_RATIO_MAT * u.camera_view * vec4(point_pos_orig, 0.0, 1.0);

    let margin_left_threshold = -1.0 + 2.0 * margin_left_norm;
    let margin_right_threshold = 1.0 - 2.0 * margin_right_norm;
    let margin_top_threshold = 1.0 - 2.0 * margin_top_norm;
    let margin_bottom_threshold = -1.0 + 2.0 * margin_bottom_norm;

    // TODO: do more clipping in the fragment shader to account for points on the boundaries.
    if (point_pos_to_ndc.x < margin_left_threshold ||
        point_pos_to_ndc.x > margin_right_threshold ||
        point_pos_to_ndc.y < margin_bottom_threshold ||
        point_pos_to_ndc.y > margin_top_threshold) {
        // Point is completely outside the plot area; move it off-screen.
        let offscreen_pos = vec4f(0.0, 0.0, 0.0, -1.0);
        var out_to_clip: VSOut;
        out_to_clip.position = offscreen_pos;
        out_to_clip.color = u.color;
        out_to_clip.quad_pos = vec2<f32>(0.0, 0.0);
        out_to_clip.instance_index = instance_index;
        out_to_clip.valid_bounds = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return out_to_clip;
    }


    let pos = vec4f(point_pos_to_ndc.x * 1.0 + (corner.x * point_size_ndc.x), point_pos_to_ndc.y * 1.0 + (corner.y * point_size_ndc.y), 0.0, 1.0);

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
