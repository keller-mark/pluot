// Resolves a channel's `stroke_width` -- expressed in screen pixels, data
// (world) units, or as a fraction of the layer height, per
// `u.stroke_width_unit_mode` -- into the mask-texel units that
// `bitmask_is_edge` measures in. This is purely a change of units: the result
// is free to be fractional, and nothing here depends on the mask's
// dimensions, only on the model matrix, camera and viewport.
//
// The mask quad's vertices are mask-texel positions pushed through
// `u.model_matrix` (which maps mask-texel space into world space) and then,
// in data positioning mode, the camera/aspect-ratio pipeline. So one texel
// spans `world_per_texel` world units and `px_per_texel` screen pixels, and
// converting a width into texels is a division by whichever of those matches
// the width's unit mode. As in the stroked polygon/curve layers, widths are
// measured relative to the Y axis. Injected once (not templated per-channel),
// regardless of channel count; assumes `translate`, `scale`,
// `get_aspect_ratio_mat` and the layer's `Uniforms` struct are in scope.
fn bitmask_stroke_width_texels(stroke_width: f32) -> f32 {
    // World-space Y extent of a single mask texel. w = 0, so the
    // model_matrix's translation cancels out (this is a size, not a position).
    let texel_world = u.model_matrix * vec4<f32>(1.0, 1.0, 0.0, 0.0);
    let world_per_texel = abs(texel_world.y);

    if (u.stroke_width_unit_mode == 1u) {
        // Data-unit width: camera-independent, because the mask itself scales
        // with the camera. Note this divides by the model_matrix rather than
        // multiplying by it as the stroked polygon/curve layers do: there
        // model_matrix maps data units to world units, here it maps mask
        // texels to world units. `BitmaskLayer::new` rejects this mode unless
        // the layer's Y axis is positioned in data units.
        return select(stroke_width / world_per_texel, 0.0, world_per_texel == 0.0);
    }

    // Screen-pixel Y extent of a single mask texel, which depends on how the
    // quad is positioned (mirrors the branches in vs_main).
    let layer_h = u.layer_size.y;
    var px_per_texel = 0.0;
    if (u.data_unit_mode_y == 1u) {
        // Data mode: run the texel-sized delta through the same pipeline as
        // positions (again with w = 0, so translations cancel).
        let aspect = u.layer_size.x / layer_h;
        let NORM_TO_NDC = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
        let NDC_TO_NORM = translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0);
        let ASPECT_RATIO_MAT = get_aspect_ratio_mat(aspect, u.aspect_ratio_mode, u.aspect_ratio_alignment_mode);
        let mvp = ASPECT_RATIO_MAT * u.camera_view;
        let texel_norm = (NDC_TO_NORM * mvp * NORM_TO_NDC) * texel_world;
        px_per_texel = abs(texel_norm.y) * layer_h;
    } else if (u.data_unit_mode_y == 2u) {
        // Normalized mode: post-model_matrix Y is a (0 to 1) fraction of the
        // layer height.
        px_per_texel = world_per_texel * layer_h;
    } else {
        // Pixel mode: post-model_matrix Y is already in screen pixels.
        px_per_texel = world_per_texel;
    }

    // Normalized-unit width: a fraction (0 to 1) of the layer height, which
    // pixel-unit width already is in absolute terms.
    let stroke_width_px = select(stroke_width, stroke_width * layer_h, u.stroke_width_unit_mode == 2u);
    return select(stroke_width_px / px_per_texel, 0.0, px_per_texel == 0.0);
}
