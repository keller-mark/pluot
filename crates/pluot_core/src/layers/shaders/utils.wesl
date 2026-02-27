fn rotate_z(angle_deg: f32) -> mat4x4<f32> {
    let angle_rad = (-1.0 * angle_deg) * 3.14159265359 / 180.0; // Convert degrees to radians
    let c = cos(angle_rad);
    let s = sin(angle_rad);
    return mat4x4<f32>(
        vec4<f32>(c, s, 0.0, 0.0),
        vec4<f32>(-s, c, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0)
    );
}

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
