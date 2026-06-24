// Full-screen post-process pass that converts the premultiplied-alpha output of
// the layered render pass back into straight (un-premultiplied) alpha.

@group(0) @binding(0) var src_tex: texture_2d<f32>;

// Emit a single oversized triangle that covers the whole framebuffer. Using
// three vertices (rather than a quad) avoids a diagonal seam and needs no
// vertex/index buffers.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(frag_coord.xy);
    let c = textureLoad(src_tex, coord, 0);
    var rgb = c.rgb;
    // Guard against divide-by-zero; for a == 1.0 the divide is a no-op.
    if (c.a > 0.0) {
        rgb = rgb / c.a;
    }
    return vec4<f32>(rgb, c.a);
}
