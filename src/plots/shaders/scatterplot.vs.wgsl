struct Uniforms {
    camera_view: mat4x4<f32>,
    point_size_px: f32,   // diameter in pixels
    _pad0: f32,
    viewport_size: vec2<f32>, // (width, height) in pixels
    color: vec4<f32>,     // rgba color for points
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> x_coords: array<f32>;
@group(0) @binding(2) var<storage, read> y_coords: array<f32>;



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
    
    // Convert to NDC
    let center_ndc = clip_space_position.xy / clip_space_position.w;
    
    /*
    // Snap to pixel grid to avoid sub-pixel jitter when zooming/panning
    let pixel_pos = vec2<f32>(0.5, 0.5) * (ndc_position + vec2<f32>(1.0, 1.0)) * u.viewport_size;

    pixel_pos = floor(pixel_pos + 0.5); // Snap to nearest pixel
    let snapped_position = (pixel_pos / vec2<f32>(u.viewport_size.x, u.viewport_size.y)) * 2.0 - 1.0;
    */

    // Convert desired pixel radius to NDC
    let radius_px = 0.5 * u.point_size_px;
    // pixels -> NDC: ndc_per_px = 2 / viewport
    let ndc_per_px = 2.0 / u.viewport_size;
    let radius_ndc = vec2<f32>(radius_px * ndc_per_px.x, radius_px * ndc_per_px.y);

    // Pick corner of quad and place around center
    let corner = QUAD[vertex_index & 3u]; // vertex_index % 4
    let offset_ndc = vec2<f32>(corner.x * radius_ndc.x, corner.y * radius_ndc.y);

    var out: VSOut;
    out.position = vec4<f32>(center_ndc + offset_ndc, 0.0, 1.0);
    out.color = u.color;
    out.quad_pos = corner; // pass unit quad position for circular masking
    out.instance_index = instance_index;
    return out;
}