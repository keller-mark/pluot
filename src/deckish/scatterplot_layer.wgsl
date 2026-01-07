struct Uniforms {
    viewport_size: vec2<f32>, // (width, height) in pixels
    plot_margin: vec4<f32>, // (top | right | bottom | left) in pixels
    camera_view: mat4x4<f32>,
    point_size_px: f32,   // diameter in pixels
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
    let p = vec2<f32>(x_coords[instance_index], y_coords[instance_index]);

    // View aspect ratio
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1271C5-L1271C52
    let viewport_w = u.viewport_size.x;
    let viewport_h = u.viewport_size.y;
    let view_aspect_ratio = viewport_w / viewport_h;

    // Model-view-projection matrix
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1582
    let projection: mat4x4<f32> = mat4x4<f32>(
        vec4<f32>(1.0 / view_aspect_ratio, 0.0, 0.0, 0.0), // Column 0
        vec4<f32>(0.0, 1.0, 0.0, 0.0), // Column 1
        vec4<f32>(0.0, 0.0, 1.0, 0.0), // Column 2
        vec4<f32>(0.0, 0.0, 0.0, 1.0), // Column 3
    );
    let model_view_projection = projection * u.camera_view;

    // Compute clip space position
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/point.vs#L48
    let clip_space_position = model_view_projection * vec4<f32>(p.x, p.y, 0.0, 1.0);

    // Convert to NDC, accounting for plot margins.
    // (We would stop here to compute center_ndc if not accounting for margins).
    let center_ndc_unconstrained = clip_space_position.xy / clip_space_position.w;

    // The plot is being rendered into a sub-region of the viewport, defined by the margins.
    // Convert margins from pixels to NDC
    let margin_ndc = u.plot_margin * (2.0 / vec4<f32>(u.viewport_size.yx, u.viewport_size.yx));
    let margin_top = margin_ndc.x;
    let margin_right = margin_ndc.y;
    let margin_bottom = margin_ndc.z;
    let margin_left = margin_ndc.w;

    // Calculate the scale factor (size of available region / size of full region)
    let scale = vec2<f32>(1.0 - (margin_left + margin_right), 1.0 - (margin_top + margin_bottom));

    // Calculate translation to center the scaled coordinates in the available region
    // The available region spans from [-1 + margin_left, 1 - margin_right] in X
    // and [-1 + margin_bottom, 1 - margin_top] in Y
    let translate = vec2<f32>(margin_left - margin_right, margin_bottom - margin_top);

    let center_ndc = center_ndc_unconstrained * scale + translate;

    // We now may have points which are positioned in the margins,
    // especially if the user has zoomed into the scatterplot.
    // We need to define the valid rendering region, accounting for margins.
    // Convert desired pixel radius to NDC
    let radius_px = 0.5 * u.point_size_px;
    // pixels -> NDC: ndc_per_px = 2 / viewport
    let ndc_per_px = 2.0 / u.viewport_size;
    let radius_ndc = vec2<f32>(radius_px * ndc_per_px.x, radius_px * ndc_per_px.y);

    // Define the valid rendering region, accounting for margins.
    let valid_min = vec2<f32>(-1.0 + margin_left, -1.0 + margin_bottom);
    let valid_max = vec2<f32>(1.0 - margin_right, 1.0 - margin_top);

    // REVISED: Check if the entire point (center + radius) is completely outside the valid region.
    // Only clip points that are entirely outside - partial points should be rendered.
    let point_min = center_ndc - radius_ndc;
    let point_max = center_ndc + radius_ndc;

    if (point_max.x < valid_min.x || point_min.x > valid_max.x ||
        point_max.y < valid_min.y || point_min.y > valid_max.y) {
        // Point is completely outside the valid region, clip it entirely
        var out: VSOut;
        // Using w < 0 clips the vertex
        out.position = vec4<f32>(0.0, 0.0, 0.0, -1.0);
        out.color = u.color;
        out.quad_pos = vec2<f32>(0.0);
        out.instance_index = instance_index;
        out.valid_bounds = vec4<f32>(valid_min, valid_max);
        return out;
    }

    /*
    // Snap to pixel grid to avoid sub-pixel jitter when zooming/panning
    let pixel_pos = vec2<f32>(0.5, 0.5) * (ndc_position + vec2<f32>(1.0, 1.0)) * u.viewport_size;

    pixel_pos = floor(pixel_pos + 0.5); // Snap to nearest pixel
    let snapped_position = (pixel_pos / vec2<f32>(u.viewport_size.x, u.viewport_size.y)) * 2.0 - 1.0;
    */

    // Pick corner of quad and place around center
    let corner = QUAD[vertex_index & 3u]; // vertex_index % 4
    let offset_ndc = vec2<f32>(corner.x * radius_ndc.x, corner.y * radius_ndc.y);

    var out: VSOut;
    out.position = vec4<f32>(center_ndc + offset_ndc, 0.0, 1.0);
    out.color = u.color;
    // Pass quad position in [0, 1] range for fragment shader.
    out.quad_pos = (corner + 1.0) * 0.5;
    out.instance_index = instance_index;
    // Pass valid bounds to fragment shader for per-fragment clipping
    out.valid_bounds = vec4<f32>(valid_min, valid_max);
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

    let category_color = get_categorical_color(labels_coords[instance_index]);

    var out: FSOut;
    // Output premultiplied alpha to work with PREMULTIPLIED_ALPHA blending
    out.color = vec4<f32>(category_color.rgb * alpha, alpha);
    return out;
}
