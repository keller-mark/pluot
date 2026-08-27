// Inspired by the Vitessce/DeckGL BitmaskLayer.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/layers/BitmaskLayerBeta.js

// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
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


// Requires `scale` and `translate` to also be injected into the same module.
fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: u32, aspect_ratio_alignment_mode: u32) -> mat4x4<f32> {
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

    // To handle aspect_ratio_alignment_mode, we compute the required translation.
    // After scale(sx, sy), the data axis spans [-sx, +sx] in NDC.
    // Center (default): no translation needed.
    // Start: We shift so the start edge aligns to -1. So, tx = sx - 1
    // End: We shift so the end edge aligns to +1.     So, tx = 1 - sx
    // When the scaling is 1.0, both formulas yield 0.
    var x_translation_for_aspect_ratio_alignment_mode = 0.0;
    var y_translation_for_aspect_ratio_alignment_mode = 0.0;
    if (aspect_ratio_alignment_mode == 1u) {
        // start
        x_translation_for_aspect_ratio_alignment_mode = x_scale_for_aspect_ratio_mode - 1.0;
        y_translation_for_aspect_ratio_alignment_mode = y_scale_for_aspect_ratio_mode - 1.0;
    } else if (aspect_ratio_alignment_mode == 2u) {
        // end
        x_translation_for_aspect_ratio_alignment_mode = 1.0 - x_scale_for_aspect_ratio_mode;
        y_translation_for_aspect_ratio_alignment_mode = 1.0 - y_scale_for_aspect_ratio_mode;
    }

    return translate(
        x_translation_for_aspect_ratio_alignment_mode,
        y_translation_for_aspect_ratio_alignment_mode,
        0.0
    ) * scale(
        x_scale_for_aspect_ratio_mode,
        y_scale_for_aspect_ratio_mode,
        1.0
    );
}


// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
// Used to read the shared multi-channel mask texture (indexed by pixel
// position and per-channel stride) and any per-channel color-mode value/palette
// textures (indexed by object id), each of which is a flat array reshaped into
// rows by `NumericData::create_data_texture`.
// Map a flat element index into 2D texel coordinates for a single-channel data
// texture whose flat array was reshaped into rows of `width` texels: element
// `idx` lives at texel `(idx % width, idx / width)`. See
// `NumericData::create_data_texture`.
fn flat_texel_coord(idx: u32, width: u32) -> vec2<u32> {
  return vec2<u32>(idx % width, idx / width);
}


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
@group(0) @binding(1) var mask_data: texture_2d<u32>;

// bitmask_sample(channel_index, px) / bitmask_is_edge(...) /
// bitmask_stroke_width_texels(...): ordinary (not per-channel-templated)
// helper functions used by the channel loop in fs_main below. See
// `crate::shader_modules::bitmask_channel`.
// Loads the bitmask value at pixel `px` for one channel of the shared,
// multi-channel `mask_data` texture (see `BitmaskLayer`), using
// `u.x_stride`/`u.y_stride`/`u.c_stride` to locate that channel's slice
// within the flat array -- mirrors `BitmapLayer`'s stride-based indexing.
// Injected once (not templated per-channel), regardless of channel count.
// Assumes `mask_data` and `flat_texel_coord` are already in scope.
fn bitmask_sample(channel_index: u32, px: vec2<u32>) -> i32 {
    let idx = px.y * u.y_stride + px.x * u.x_stride + channel_index * u.c_stride;
    let mask_tex_width = textureDimensions(mask_data).x;
    return i32(textureLoad(mask_data, flat_texel_coord(idx, mask_tex_width), 0).x);
}


// Approximate object-boundary test: true if the point `px` (whose object id is
// `raw_label`) has a differently-labeled neighbor within `stroke_width` texels,
// in any of 8 directions. Used to render outline-only channels
// (`BitmaskChannelSettings::filled == false`). Injected once (not templated
// per-channel), regardless of channel count. Depends on `bitmask_sample`
// also being injected.
//
// `px` is the *continuous* position of the fragment in mask-texel space, not
// the integer texel it falls in, and `stroke_width` may be fractional.
// This keeps the outline's thickness independent of the mask's resolution.
//
// Note the diagonal offsets reach `stroke_width * sqrt(2)`, so the band bulges
// somewhat at corners; this samples a square, not a disc.
fn bitmask_is_edge(
    channel_index: u32,
    px: vec2<f32>,
    raw_label: i32,
    img_w: u32,
    img_h: u32,
    stroke_width: f32,
) -> bool {
    let off = max(stroke_width, 0.0);
    let max_x = f32(img_w) - 1.0;
    let max_y = f32(img_h) - 1.0;
    let deltas = array<vec2<f32>, 8>(
        vec2<f32>(off, 0.0), vec2<f32>(-off, 0.0),
        vec2<f32>(0.0, off), vec2<f32>(0.0, -off),
        vec2<f32>(off, off), vec2<f32>(-off, off),
        vec2<f32>(off, -off), vec2<f32>(-off, -off),
    );
    for (var k = 0; k < 8; k = k + 1) {
        let n = px + deltas[k];
        let nx = u32(clamp(floor(n.x), 0.0, max_x));
        let ny = u32(clamp(floor(n.y), 0.0, max_y));
        let nval = bitmask_sample(channel_index, vec2<u32>(nx, ny));
        if (nval != raw_label) {
            return true;
        }
    }
    return false;
}


// Resolves a channel's `stroke_width` -- expressed in screen pixels, data
// (world) units, or as a fraction of the layer height (normalized units), per
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
// the width's unit mode. Injected once (not templated per-channel),
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


// Quantitative colormap function(s) used by any channel in `Quantitative`
// color mode, deduplicated by name so a colormap shared by multiple channels
// is only defined once in this shader module.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js

fn viridis(x_1: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.26666666666666666,0.00392156862745098,0.32941176470588235,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.2784313725490196,0.17254901960784313,0.47843137254901963,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.23137254901960785,0.3176470588235294,0.5450980392156862,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.17254901960784313,0.44313725490196076,0.5568627450980392,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.12941176470588237,0.5647058823529412,0.5529411764705883,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.15294117647058825,0.6784313725490196,0.5058823529411764,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.3607843137254902,0.7843137254901961,0.38823529411764707,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.6666666666666666,0.8627450980392157,0.19607843137254902,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.9921568627450981,0.9058823529411765,0.1450980392156863,1.0);
  let a0 = smoothstep(e0,e1,x_1);
  let a1 = smoothstep(e1,e2,x_1);
  let a2 = smoothstep(e2,e3,x_1);
  let a3 = smoothstep(e3,e4,x_1);
  let a4 = smoothstep(e4,e5,x_1);
  let a5 = smoothstep(e5,e6,x_1);
  let a6 = smoothstep(e6,e7,x_1);
  let a7 = smoothstep(e7,e8,x_1);
  return max(mix(v0,v1,a0)*step(e0,x_1)*step(x_1,e1),
    max(mix(v1,v2,a1)*step(e1,x_1)*step(x_1,e2),
    max(mix(v2,v3,a2)*step(e2,x_1)*step(x_1,e3),
    max(mix(v3,v4,a3)*step(e3,x_1)*step(x_1,e4),
    max(mix(v4,v5,a4)*step(e4,x_1)*step(x_1,e5),
    max(mix(v5,v6,a5)*step(e5,x_1)*step(x_1,e6),
    max(mix(v6,v7,a6)*step(e6,x_1)*step(x_1,e7),mix(v7,v8,a7)*step(e7,x_1)*step(x_1,e8)
  )))))));
}


// Per-channel getters, five per channel: `get_channel_fill_color_N` /
// `get_channel_stroke_color_N` (-> vec3<f32>) assembled according to that
// channel's fill/stroke `ColorMode`, and `get_channel_fill_opacity_N` /
// `get_channel_stroke_opacity_N` / `get_channel_stroke_width_N` (-> f32)
// assembled according to its `OpacityMode`/`SizeMode`. Each mirrors the
// layer-wide equivalent in `crate::color_mode`/`crate::scalar_mode`,
// specialized to a unique function name and texture bindings per (channel,
// property) pair, since WGSL has no per-instance function dispatch and every
// channel may resolve every property differently.
// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform. Templated per
// (channel, fill/stroke) pair, hence the two-part function name.
fn get_channel_fill_color_0(label_index: u32) -> vec3<f32> {
  return u.channels[0].fill_color_static.rgb;
}

// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform. Templated per
// (channel, fill/stroke) pair, hence the two-part function name.
fn get_channel_stroke_color_0(label_index: u32) -> vec3<f32> {
  return u.channels[0].stroke_color_static.rgb;
}

// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_fill_opacity_0(label_index: u32) -> f32 {
  return u.channels[0].fill_opacity;
}

// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_stroke_opacity_0(label_index: u32) -> f32 {
  return u.channels[0].stroke_opacity;
}

// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_stroke_width_0(label_index: u32) -> f32 {
  return u.channels[0].stroke_width;
}

fn get_channel_is_filtered_in_0(instance_index: u32) -> bool {
    return true;
}

fn get_channel_is_selected_in_0(instance_index: u32) -> bool {
    return true;
}

// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform. Templated per
// (channel, fill/stroke) pair, hence the two-part function name.
fn get_channel_fill_color_1(label_index: u32) -> vec3<f32> {
  return u.channels[1].fill_color_static.rgb;
}

// BitmaskLayer per-channel ColorMode::Quantitative — a per-object scalar
// feature value, indexed by object id (`label_index`), normalized into 0-1
// using the channel's (min, max) domain, then mapped through a continuous
// colormap. The colormap function's source and name are injected by
// ShaderBuilder as placeholders below (not spelled out here, to avoid the
// literal placeholder text itself being matched and substituted). Depends on
// `flat_texel_coord` being injected.
@group(0) @binding(2) var channel_stroke_color_values_1: texture_2d<f32>;

fn get_channel_stroke_color_1(label_index: u32) -> vec3<f32> {
  var x = f32(textureLoad(channel_stroke_color_values_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_values_1).x), 0).x);
  let lo = u.channels[1].stroke_color_domain.x;
  let hi = u.channels[1].stroke_color_domain.y;
  x = clamp((x - lo) / max(hi - lo, 1e-20), 0.0, 1.0);
  if (u.channels[1].stroke_color_reverse == 1u) {
    x = 1.0 - x;
  }
  return viridis(x).rgb;
}

// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_fill_opacity_1(label_index: u32) -> f32 {
  return u.channels[1].fill_opacity;
}

// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_stroke_opacity_1(label_index: u32) -> f32 {
  return u.channels[1].stroke_opacity;
}

// BitmaskLayer per-channel SizeMode::InstancedSize /
// OpacityMode::InstancedOpacity — one value per object, read from a value
// texture indexed by object id (`label_index`). Objects past the end of the
// array read the texture's zero padding, i.e. no stroke / full transparency.
// Depends on `flat_texel_coord` being injected.
@group(0) @binding(3) var channel_stroke_width_values_1: texture_2d<f32>;

fn get_channel_stroke_width_1(label_index: u32) -> f32 {
  return f32(textureLoad(channel_stroke_width_values_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_width_values_1).x), 0).x);
}

fn get_channel_is_filtered_in_1(instance_index: u32) -> bool {
    return true;
}

fn get_channel_is_selected_in_1(instance_index: u32) -> bool {
    return true;
}


// get_channel_fill_color(channel_index, label_index) and its siblings: each
// dispatches to the per-channel function above matching `channel_index`,
// including `get_channel_is_filtered_in`/`get_channel_is_selected_in`, which
// dispatch to each channel's filtering/selection criteria predicate. See
// `crate::shader_modules::bitmask_channel::CHANNEL_COLOR_DISPATCH` /
// `CHANNEL_SCALAR_DISPATCH` / `CHANNEL_BOOL_DISPATCH`.
// The switch case list below is substituted in with one case per
// channel, which calls that channel's generated color getter.
// Depends on the getter functions (see `get_channel_color`) also being injected.
fn get_channel_fill_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        case 0u: { return get_channel_fill_color_0(label_index); }
        case 1u: { return get_channel_fill_color_1(label_index); }
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}

// The switch case list below is substituted in with one case per
// channel, which calls that channel's generated color getter.
// Depends on the getter functions (see `get_channel_color`) also being injected.
fn get_channel_stroke_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        case 0u: { return get_channel_stroke_color_0(label_index); }
        case 1u: { return get_channel_stroke_color_1(label_index); }
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}

// Scalar counterpart of `channel_color_dispatch`: dispatches one of the
// per-channel scalar getters (fill opacity, stroke opacity or stroke width).
fn get_channel_fill_opacity(channel_index: u32, label_index: u32) -> f32 {
    switch (channel_index) {
        case 0u: { return get_channel_fill_opacity_0(label_index); }
        case 1u: { return get_channel_fill_opacity_1(label_index); }
        default: { return 0.0; }
    }
}

// Scalar counterpart of `channel_color_dispatch`: dispatches one of the
// per-channel scalar getters (fill opacity, stroke opacity or stroke width).
fn get_channel_stroke_opacity(channel_index: u32, label_index: u32) -> f32 {
    switch (channel_index) {
        case 0u: { return get_channel_stroke_opacity_0(label_index); }
        case 1u: { return get_channel_stroke_opacity_1(label_index); }
        default: { return 0.0; }
    }
}

// Scalar counterpart of `channel_color_dispatch`: dispatches one of the
// per-channel scalar getters (fill opacity, stroke opacity or stroke width).
fn get_channel_stroke_width(channel_index: u32, label_index: u32) -> f32 {
    switch (channel_index) {
        case 0u: { return get_channel_stroke_width_0(label_index); }
        case 1u: { return get_channel_stroke_width_1(label_index); }
        default: { return 0.0; }
    }
}

// Boolean counterpart of `channel_scalar_dispatch`/`channel_color_dispatch`:
// dispatches one of the per-channel filtering/selection predicates
// (is_filtered_in or is_selected_in). Defaults to true (include) for a
// channel index out of range, matching the empty-criteria "everything
// included" semantics. Depends on the getter functions (see
// `crate::emphasis_mode::prepare_emphasis_criteria`) also being injected.
fn get_channel_is_filtered_in(channel_index: u32, label_index: u32) -> bool {
    switch (channel_index) {
        case 0u: { return get_channel_is_filtered_in_0(label_index); }
        case 1u: { return get_channel_is_filtered_in_1(label_index); }
        default: { return true; }
    }
}

// Boolean counterpart of `channel_scalar_dispatch`/`channel_color_dispatch`:
// dispatches one of the per-channel filtering/selection predicates
// (is_filtered_in or is_selected_in). Defaults to true (include) for a
// channel index out of range, matching the empty-criteria "everything
// included" semantics. Depends on the getter functions (see
// `crate::emphasis_mode::prepare_emphasis_criteria`) also being injected.
fn get_channel_is_selected_in(channel_index: u32, label_index: u32) -> bool {
    switch (channel_index) {
        case 0u: { return get_channel_is_selected_in_0(label_index); }
        case 1u: { return get_channel_is_selected_in_1(label_index); }
        default: { return true; }
    }
}


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

        // Filter-excluded objects are treated the same as "no object" for
        // this channel: not drawn, not picked. See
        // `.claude/skills/pluot-filter-select-highlight`.
        if (!get_channel_is_filtered_in(channel_index, label_index)) {
            continue;
        }
        // Filter-included but selection-excluded ("background") objects still
        // render, but de-emphasized with `ch.background_fill_color` /
        // `ch.background_stroke_color` in place of their configured fill/stroke
        // color.
        let is_selected = get_channel_is_selected_in(channel_index, label_index);

        // The outline band is the outermost part of an object's interior, so
        // the stroke and the fill cover disjoint regions and this pixel takes
        // one or the other -- never a blend of both, as in `PointLayer`. A
        // channel that is stroked but not filled leaves the interior
        // transparent; one that is filled but not stroked draws no band.
        //
        // The edge test measures in mask texels, so this channel's stroke
        // width is resolved out of screen-pixel/data/normalized units first.
        // The result may be fractional, which the test handles.
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
