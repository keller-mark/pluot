// Inspired by the Vitessce/DeckGL BitmaskLayer.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/layers/BitmaskLayerBeta.js

// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
// Used to read the shared multi-channel mask texture (indexed by pixel
// position and per-channel stride) and any per-channel color-mode value/palette
// textures (indexed by object id), each of which is a flat array reshaped into
// rows by `NumericData::create_data_texture`.
{{flat_texel_coord}}

// The fill and the stroke each carry their own ColorMode and OpacityMode, so
// most fields below come in a `fill_`/`stroke_` pair; the `<prefix>_color_*`
// and `<prefix>_opacity` names are also the property names the per-channel
// getter templates are specialized with (see `crate::shader_modules`).
struct Channel {
    fill_color_mode: u32,          // see ColorMode::shader_mode()
    fill_color_static: vec4<f32>,  // rgba color used by the UniformRgb mode
    fill_color_reverse: u32,       // 1 = reverse the quantitative colormap
    fill_color_domain: vec2<f32>,  // (min, max) normalization domain for quantitative mode

    stroke_color_mode: u32,          // as above, for the outline
    stroke_color_static: vec4<f32>,
    stroke_color_reverse: u32,
    stroke_color_domain: vec2<f32>,

    fill_opacity: f32,   // opacity used by the UniformOpacity mode
    stroke_opacity: f32,
    stroke_width: f32,   // outline thickness used by the UniformSize mode, in the units given by u.stroke_width_unit_mode

    filled: u32,   // 1 = fill object interiors
    stroked: u32,  // 1 = draw an outline along object boundaries

    background_fill_color: vec4<f32>,   // rgba fill color used for filter-included, selection-excluded ("background") objects
    background_stroke_color: vec4<f32>, // rgba stroke color used for filter-included, selection-excluded ("background") objects
};

struct Uniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32, // 0: pixel units, 1: data units, 2: normalized (0-1) units
    data_unit_mode_y: u32, // 0: pixel units, 1: data units, 2: normalized (0-1) units
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end

    img_size: vec2<f32>, // (img_width, img_height) in pixels
    pixel_offset: vec2<f32>, // (x_offset, y_offset) in pixels, for tiling support

    model_matrix: mat4x4<f32>,

    opacity: f32, // overall layer opacity multiplier

    // How to interpret every channel's `stroke_width`.
    stroke_width_unit_mode: u32, // 0: px units, 1: data coordinate system units, 2: normalized (0-1) units

    // Strides for each dimension (in units of elements), allowing the shader to
    // index into the flat `mask_data` buffer regardless of the dimension
    // ordering (e.g. CYX vs YXC) -- mirrors `BitmapLayer`'s uniforms.
    x_stride: u32,
    y_stride: u32,
    c_stride: u32,

    num_channels: u32,
    // See "runtime sized arrays" info
    // Reference: https://webgpufundamentals.org/webgpu/lessons/webgpu-wgsl.html#runtime-sized-arrays
    channels: array<Channel>,
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

@group(0) @binding(0) var<storage, read> u: Uniforms;

// The mask data for every channel is uploaded as a single-channel (red-only)
// 2D texture holding the flat data reshaped into rows (element `idx` at texel
// `(idx % width, idx / width)`). The data is NOT reordered on the CPU, so the
// shader still indexes with per-dimension strides (see `x_stride`/`y_stride`/
// `c_stride`) before mapping the flat index back to 2D texel coordinates. The
// texture's sampled type is injected at runtime by the shader-module system
// (see `crate::shader_modules`), same as `BitmapLayer`'s `img_data`.
@group(0) @binding(1) var mask_data: texture_2d<{{mask_data_dtype}}>;

// bitmask_sample(channel_index, px) / bitmask_is_edge(...) /
// bitmask_stroke_width_texels(...): ordinary (not per-channel-templated)
// helper functions used by the channel loop in fs_main below. See
// `crate::shader_modules::bitmask_channel`.
{{bitmask_sample}}

{{bitmask_is_edge}}

{{bitmask_stroke_width_texels}}

// Quantitative colormap function(s) used by any channel in `Quantitative`
// color mode, deduplicated by name so a colormap shared by multiple channels
// is only defined once in this shader module.
{{colormap_functions}}

// Per-channel getters, seven per channel: `get_channel_fill_color_N` /
// `get_channel_stroke_color_N` (-> vec3<f32>) assembled according to that
// channel's fill/stroke `ColorMode`, `get_channel_fill_opacity_N` /
// `get_channel_stroke_opacity_N` / `get_channel_stroke_width_N` (-> f32)
// assembled according to its `OpacityMode`/`SizeMode`, and
// `get_channel_is_filtered_in_N` / `get_channel_is_selected_in_N` (-> bool)
// assembled according to its `filtering_criteria`/`selection_criteria` (see
// `.claude/skills/pluot-filter-select-highlight`). Each mirrors the
// layer-wide equivalent in `crate::color_mode`/`crate::scalar_mode`/
// `crate::emphasis_mode`, specialized to a unique function name and texture
// bindings per (channel, property) pair, since WGSL has no per-instance
// function dispatch and every channel may resolve every property differently.
{{channel_functions}}

// get_channel_fill_color(channel_index, label_index) and its siblings: each
// dispatches to the per-channel function above matching `channel_index`. See
// `crate::shader_modules::bitmask_channel::CHANNEL_COLOR_DISPATCH` /
// `CHANNEL_SCALAR_DISPATCH` / `CHANNEL_BOOL_DISPATCH`.
{{channel_dispatchers}}

// A quad that covers the full viewport in Normalized Device Coordinates (NDC).
// The corresponding texture coordinates (UVs) for each vertex.
// 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0), // bottom-left
    vec2<f32>(1.0, 0.0), // bottom-right
    vec2<f32>(0.0,  1.0), // top-left
    vec2<f32>(1.0,  1.0)  // top-right
);

const TEX_COORDS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 1.0), // bottom-left
    vec2<f32>(1.0, 1.0), // bottom-right
    vec2<f32>(0.0, 0.0), // top-left
    vec2<f32>(1.0, 0.0)  // top-right
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32
) -> VSOut {
    // Get the texture coordinate for the current vertex.
    let uv = TEX_COORDS[vertex_index];
    // Obtain a position for this vertex in (0 to 1) normalized space.
    let vertex_pos_norm = QUAD[vertex_index];
    let vertex_pos_px = u.model_matrix * vec4f(
        vertex_pos_norm.x * u.img_size.x + u.pixel_offset.x,
        vertex_pos_norm.y * u.img_size.y + u.pixel_offset.y,
        0.0,
        1.0
    );

    // Positioning mirrors `BitmapLayer`: pixel/normalized modes place the quad
    // relative to the layer bounds (camera-independent); data mode runs the
    // quad through the camera/aspect-ratio pipeline. See `bitmap_layer.wgsl`
    // for the full rationale.

    let layer_width_px = u.layer_size.x;
    let layer_height_px = u.layer_size.y;

    let layer_aspect_ratio = layer_width_px / layer_height_px;

    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        u.aspect_ratio_mode,
        u.aspect_ratio_alignment_mode
    );

    // We operate in (0 to 1) space, since it is more intuitive.
    // We therefore need matrices to transform (0, 1) into clip space ("NDC") (-1 to 1)
    let NORM_TO_NDC_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
    // And the inverse, to convert back from NDC (-1 to 1) to normalized (0 to 1) space.
    let NDC_TO_NORM_MAT = translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0);

    var result_position_px = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var result_position_data = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if(u.data_unit_mode_x != 1u || u.data_unit_mode_y != 1u) {
        let point_pos_norm = vec2<f32>(
            select(vertex_pos_px.x / layer_width_px, vertex_pos_px.x, u.data_unit_mode_x == 2u),
            select(vertex_pos_px.y / layer_height_px, vertex_pos_px.y, u.data_unit_mode_y == 2u)
        );
        let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

        result_position_px = point_pos_ndc;

        if(u.data_unit_mode_x != 1u && u.data_unit_mode_y != 1u) {
            var out: VSOut;
            out.position = result_position_px;
            out.tex_coord = uv;
            return out;
        }
    }

    // Handle data_unit_mode == "data"
    let model_view_projection = ASPECT_RATIO_MAT * u.camera_view;

    let point_pos_norm = (
        (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT)
        * vertex_pos_px
    );
    let point_pos_ndc = NORM_TO_NDC_MAT * vec4f(point_pos_norm.xy, 0.0, 1.0);

    result_position_data = point_pos_ndc;

    if(u.data_unit_mode_x != 1u) {
        result_position_data.x = result_position_px.x;
    }
    if(u.data_unit_mode_y != 1u) {
        result_position_data.y = result_position_px.y;
    }

    var out: VSOut;
    out.position = result_position_data;
    out.tex_coord = uv;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Shared pixel-position computation: every channel is a slice of the same
    // (img_width, img_height) `mask_data` array, so this only needs to happen
    // once rather than per channel.
    // The continuous position, kept alongside the integer texel it falls in:
    // the label is read from the containing texel, but the boundary test
    // offsets this continuous position so the outline's thickness does not
    // quantize to the mask's resolution (see `bitmask_is_edge`).
    let px_f = in.tex_coord * u.img_size;
    let px = vec2<u32>(min(
        vec2<u32>(floor(px_f)),
        vec2<u32>(u.img_size) - vec2<u32>(1u, 1u)
    ));
    let img_w = u32(u.img_size.x);
    let img_h = u32(u.img_size.y);

    var out_rgb = vec3<f32>(0.0, 0.0, 0.0);
    var out_a = 0.0;
    var any_on = false;

    // Loop over every configured channel: sample this channel's slice of
    // mask_data (bitmask_sample), decide whether this pixel falls in the
    // object's outline band (bitmask_stroke_width_texels + bitmask_is_edge) or
    // its interior, and blend the resolved stroke/fill color and opacity into
    // out_rgb/out_a. num_channels and each Channel come from the storage
    // buffer, so this is a genuine runtime loop -- not unrolled/generated per
    // channel.
    for (var channel_index: u32 = 0u; channel_index < u.num_channels; channel_index = channel_index + 1u) {
        let ch = u.channels[channel_index];
        if (ch.filled == 0u && ch.stroked == 0u) {
            continue;
        }

        let raw_label = bitmask_sample(channel_index, px);
        if (raw_label == 0) {
            continue;
        }
        let label_index = u32(raw_label - 1);

        // Filter-excluded objects are not rendered at all: not as fill, not as
        // stroke, not in picking. See
        // `.claude/skills/pluot-filter-select-highlight`.
        if (!get_channel_is_filtered_in(channel_index, label_index)) {
            continue;
        }

        // The outline band is the outermost part of an object's interior, so
        // the stroke and the fill cover disjoint regions and this pixel takes
        // one or the other -- never a blend of both, as in `PointLayer`. A
        // channel that is stroked but not filled leaves the interior
        // transparent; one that is filled but not stroked draws no band.
        //
        // The edge test measures in mask texels, so this channel's stroke
        // width is resolved out of screen-pixel/data/normalized units first.
        // The result may be fractional, which the test handles.
        //
        // Filter-included but selection-excluded ("background") objects still
        // render, but de-emphasized with `ch.background_fill_color` /
        // `ch.background_stroke_color` in place of their configured fill/stroke
        // color.
        let is_selected = get_channel_is_selected_in(channel_index, label_index);
        var color: vec3<f32>;
        var alpha: f32;
        if (ch.stroked == 1u && bitmask_is_edge(
            channel_index,
            px_f,
            raw_label,
            img_w,
            img_h,
            bitmask_stroke_width_texels(get_channel_stroke_width(channel_index, label_index))
        )) {
            color = select(ch.background_stroke_color.rgb, get_channel_stroke_color(channel_index, label_index), is_selected);
            alpha = get_channel_stroke_opacity(channel_index, label_index);
        } else if (ch.filled == 1u) {
            color = select(ch.background_fill_color.rgb, get_channel_fill_color(channel_index, label_index), is_selected);
            alpha = get_channel_fill_opacity(channel_index, label_index);
        } else {
            continue;
        }

        out_rgb = mix(out_rgb, color, alpha);
        out_a = max(out_a, alpha);
        any_on = true;
    }

    // If every channel was off (background) at this pixel, discard so this
    // fragment is not considered during picking/blending.
    if (!any_on) {
        discard;
    }

    return vec4<f32>(out_rgb, out_a * u.opacity);
}
