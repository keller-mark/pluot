struct Uniforms {
    camera_view: mat4x4<f32>,
    point_size_px: f32,   // diameter in pixels
    viewport_size: vec2<f32>, // (width, height) in pixels
    color: vec4<f32>,     // rgba color for points
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(3) var<storage, read> labels_coords: array<i32>;

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
    @location(0) color_in: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
) -> FSOut {
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
