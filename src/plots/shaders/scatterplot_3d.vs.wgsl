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
@group(0) @binding(3) var<storage, read> z_coords: array<f32>;


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
    // Center of this point in 3D data space
    let p = vec3<f32>(x_coords[instance_index], y_coords[instance_index], z_coords[instance_index]);

    // View aspect ratio
    let view_aspect_ratio = u.viewport_size.x / u.viewport_size.y;

    // 3D Perspective Projection Matrix
    // Using common defaults: fov = 45 degrees, near = 0.1, far = 100.0
    let fov_y = 0.785398; // 45 degrees in radians
    let f = 1.0 / tan(fov_y / 2.0);
    let near = 0.1;
    let far = 100.0;
    let nf = 1.0 / (near - far);

    let projection = mat4x4<f32>(
        f / view_aspect_ratio, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far + near) * nf, -1.0,
        0.0, 0.0, 2.0 * far * near * nf, 0.0
    );

    let model_view_projection = projection * u.camera_view;

    // Compute clip space position for the center of the point
    let clip_space_position = model_view_projection * vec4<f32>(p, 1.0);

    // Convert desired pixel radius to NDC
    let radius_px = 0.5 * u.point_size_px;
    let ndc_per_px = 2.0 / u.viewport_size;
    let radius_ndc = vec2<f32>(radius_px * ndc_per_px.x, radius_px * ndc_per_px.y);

    // Pick corner of quad and create offset in NDC space
    let corner = QUAD[vertex_index & 3u];
    let offset_ndc = corner * radius_ndc;

    var out: VSOut;
    // The final position is the point's center in clip space,
    // with an offset applied in the XY plane. The offset is scaled by W
    // to ensure the point has a constant size in screen space (billboarding).
    out.position = vec4<f32>(
        clip_space_position.xy + offset_ndc * clip_space_position.w,
        clip_space_position.z,
        clip_space_position.w
    );

    out.color = u.color;
    // Pass quad position in [0, 1] range for fragment shader.
    out.quad_pos = (corner + 1.0) * 0.5;
    out.instance_index = instance_index;
    return out;
}
