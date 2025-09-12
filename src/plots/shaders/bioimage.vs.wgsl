// TODO: put vertex and fragment shaders in the same file, to share struct/binding definitions.
struct Channel {
    window: vec2<f32>, // (min, max) for contrast adjustment
    color: vec3<f32>,  // RGB color for the channel
};

struct Uniforms {
    camera_view: mat4x4<f32>,
    viewport_size: vec2<f32>, // (width, height) in pixels
    num_channels: u32,

    // See "runtime sized arrays" info
    // Reference: https://webgpufundamentals.org/webgpu/lessons/webgpu-wgsl.html#runtime-sized-arrays
    channels: array<Channel>,
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>, // Pass texture coordinates to fragment shader
};

@group(0) @binding(0) var<storage, read> u: Uniforms;

// A quad that covers the full viewport in Normalized Device Coordinates (NDC).
// The corresponding texture coordinates (UVs) for each vertex.
// 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0), // bottom-left
    vec2<f32>( 1.0, -1.0), // bottom-right
    vec2<f32>(-1.0,  1.0), // top-left
    vec2<f32>( 1.0,  1.0)  // top-right
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
    // Get the position and texture coordinate for the current vertex.
    let pos = QUAD[vertex_index];
    let uv = TEX_COORDS[vertex_index];

    // The image quad will be transformed by the camera view, allowing pan and zoom.
    // The quad is defined in a space from -1 to 1.
    // To make it cover the image dimensions, you might need to scale it
    // before applying the camera view if your coordinate systems require it.
    // For a simple case, we assume the camera is set up to frame the [-1, 1] quad.
    let clip_space_position = u.camera_view * vec4<f32>(pos, 0.0, 1.0);

    var out: VSOut;
    out.position = clip_space_position;
    out.tex_coord = uv;
    return out;
}
