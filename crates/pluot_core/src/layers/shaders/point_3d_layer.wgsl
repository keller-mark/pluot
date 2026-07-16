struct Point3dLayerUniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    point_radius: f32,
    point_shape_mode: u32, // 0: square; 1: circle
    color: vec4<f32>,     // rgba color for points
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) corner: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Point3dLayerUniforms;
// The X/Y/Z coordinate arrays are uploaded as single-channel (red-only) 2D
// textures holding the flat array reshaped into rows: element `idx` (the
// instance index) lives at texel `(idx % width, idx / width)`. The data is NOT
// reordered on the CPU, so the shader recomputes the 2D texel coords from the
// instance index. Each texture's sampled type is injected at runtime by the
// shader-module system (see `crate::shader_modules`) so that 8/16/32-bit data
// lives on the GPU at native width: `f32` for floating-point data, `u32` for
// unsigned, `i32` for signed. X, Y, and Z are independent and may differ in dtype.
@group(0) @binding(1) var x_coords: texture_2d<{{x_dtype}}>;
@group(0) @binding(2) var y_coords: texture_2d<{{y_dtype}}>;
@group(0) @binding(3) var z_coords: texture_2d<{{z_dtype}}>;
@group(0) @binding(4) var<storage, read> labels_coords: array<i32>;


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
    // Center of this point in 3D data space. Map the flat instance index into
    // the 2D texture the coordinates were reshaped into on upload:
    // (idx % width, idx / width). `f32(...)` is a no-op when the injected
    // sampled type is already f32, and widens u32/i32 texels to f32 otherwise.
    let x_tex_width = textureDimensions(x_coords).x;
    let y_tex_width = textureDimensions(y_coords).x;
    let z_tex_width = textureDimensions(z_coords).x;
    let p = vec3<f32>(
        f32(textureLoad(x_coords, vec2<u32>(instance_index % x_tex_width, instance_index / x_tex_width), 0).x),
        f32(textureLoad(y_coords, vec2<u32>(instance_index % y_tex_width, instance_index / y_tex_width), 0).x),
        f32(textureLoad(z_coords, vec2<u32>(instance_index % z_tex_width, instance_index / z_tex_width), 0).x)
    );

    // View aspect ratio
    let view_aspect_ratio = u.layer_size.x / u.layer_size.y;

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
    let ndc_per_px = 2.0 / u.layer_size;
    let radius_ndc = vec2<f32>(u.point_radius * ndc_per_px.x, u.point_radius * ndc_per_px.y);

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
    out.corner = corner;
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

fn linearstep(edge0: f32, edge1: f32, x: f32) -> f32 {
  return clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) color_in: vec4<f32>,
    @location(1) corner: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
) -> FSOut {

    // Handling of circle point shape mode
    // Anti-aliased circle using linearstep, based on https://github.com/flekschas/regl-scatterplot/blob/main/src/point.fs
    // Reference: https://github.com/flekschas/regl-scatterplot/blob/90f0c951233b20bebd4fd1cb15ce1c4128ce9edf/src/constants.js#L175
    var alpha = 1.0;
    if(u.point_shape_mode == 1u) {
        let dist = length(corner);
        let edge_width = fwidth(dist);
        alpha = 1.0 - smoothstep(1.0 - edge_width, 1.0 + edge_width, dist);
        if (alpha < 0.001) {
            discard;
        }
    }

    let category_color = get_categorical_color(labels_coords[instance_index]);

    var out: FSOut;
    out.color = vec4<f32>(category_color.rgb, alpha);
    return out;
}
