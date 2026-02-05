// Inspired by the DeckGL TextLayer
// Reference: https://deck.gl/docs/api-reference/layers/text-layer
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};

use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
use fontdue::{Font, FontSettings};

use crate::layers::core::{AspectRatioMode, DrawToCanvas, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams};
use crate::wgpu;
use crate::wgpu::util::DeviceExt; // This import enables usage of device.create_buffer_init
use crate::cache::{use_memo_vec_f32, use_memo_vec_i32, use_memo_internal_text_layer_data, CachedInternalTextLayerData};
use svg::node::element::Group;
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle,
    TwoColor, TwoText, TwoTextAlign, TwoTextBaseline
};
use crate::two::svg::update_svg;
use crate::layers::position_utils::get_point_position;
use crate::log;

const FONT_BYTES: &[u8] = include_bytes!("../two/fonts/Inter-Bold.ttf").as_slice();

// Cached font atlas data
#[derive(Clone)]
struct FontAtlasCache {
    font: Font,
    atlas_texture: Option<wgpu::Texture>,
    glyph_cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>, // char + font_size -> metrics + bitmap
}

thread_local! {
    static FONT_ATLAS: RefCell<Option<FontAtlasCache>> = RefCell::new(None);
}

fn get_or_init_font_atlas() -> FontAtlasCache {
    FONT_ATLAS.with(|atlas| {
        let mut atlas_ref = atlas.borrow_mut();
        if let Some(ref cached_atlas) = *atlas_ref {
            cached_atlas.clone()
        } else {
            let font = Font::from_bytes(FONT_BYTES, FontSettings::default()).expect("load font");
            let cache = FontAtlasCache {
                font,
                atlas_texture: None,
                glyph_cache: HashMap::new(),
            };
            *atlas_ref = Some(cache.clone());
            cache
        }
    })
}


// Text measurement functions
fn measure_text_width(font: &Font, text: &str, font_size: f32) -> f32 {
    let mut layout: Layout = Layout::new(CoordinateSystem::PositiveYUp);
    layout.reset(&LayoutSettings {
        max_width: None,
        max_height: None,
        ..LayoutSettings::default()
    });
    layout.append(&[font], &TextStyle::new(text, font_size, 0));

    let glyphs = layout.glyphs();
    if glyphs.is_empty() {
        return 0.0;
    }

    // Calculate the total width by finding the rightmost point
    let mut max_x = 0.0f32;
    for glyph in glyphs {
        let right_edge = glyph.x + glyph.width as f32;
        max_x = max_x.max(right_edge);
    }
    max_x
}

fn calculate_text_position(font_size: f32, text_align: TextAlignMode, text_baseline: TextBaselineMode, text_width: f32) -> (f32, f32) {
    let x = match text_align {
        TextAlignMode::Start => 0.0,
        TextAlignMode::Middle => 0.0 - text_width / 2.0,
        TextAlignMode::End => 0.0 - text_width,
    };

    let y = match text_baseline {
        // For some reason, GlyphPosition.y is always a big negative number -(font_size plus some extra pixels)
        // So we adjust accordingly here.
        TextBaselineMode::Top => font_size - font_size,
        TextBaselineMode::Middle => font_size - font_size / 2.0,
        TextBaselineMode::Alphabetic => font_size - font_size / 2.0, // TODO
        TextBaselineMode::Bottom => font_size,
    };

    (x, y)
}

fn parse_color(color: &TwoColor) -> [f32; 4] {
    match color {
        TwoColor::Rgb((r, g, b)) => {
            let r = *r as f32 / 255.0;
            let g = *g as f32 / 255.0;
            let b = *b as f32 / 255.0;
            [r, g, b, 1.0]
        }
        TwoColor::Rgba((r, g, b, a)) => {
            let r = *r as f32 / 255.0;
            let g = *g as f32 / 255.0;
            let b = *b as f32 / 255.0;
            let a = *a as f32 / 255.0;
            [r, g, b, a]
        }
    }
}

// Configurable padding around each glyph to prevent texture bleeding
const PADDING: usize = 1;




#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TextAlignMode {
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TextBaselineMode {
    Top,
    Middle,
    Bottom,
    Alphabetic,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TextLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode: UnitsMode, // Units of x/y positions.
    pub text_size: f32,
    pub text_size_unit_mode: UnitsMode, // Units of the font size.
    pub text_align_mode: TextAlignMode,
    pub text_baseline_mode: TextBaselineMode,
    pub text_rotation: Option<f32>, // Rotation in degrees

    // TODO(ref): pass in references instead of owned Vecs?
    // Would this cause issues when using serde to create layers based on JSON params?
    // TODO: improve naming here - should these be "x", "y", etc?
    pub x_vec: Vec<f32>, // TODO: generalize to other numeric dtypes?
    pub y_vec: Vec<f32>,
    pub text_vec: Vec<String>,
}

// TODO: defaults for params?


// Internal representation for TextLayer and its "descendant" layers.
pub struct TextLayerData {
    pub x_arr: Vec<f32>,
    pub y_arr: Vec<f32>,
    pub text_arr: Vec<String>,
}

// Re-export the cached internal data type for convenience.
pub type InternalTextLayerData = CachedInternalTextLayerData;


pub struct TextLayer {
    view_params: ViewParams,
    layer_params: TextLayerParams,
    // TODO: getters?

    // Data may be None prior to runninng prepare().
    data: Option<TextLayerData>,
    // NOTE: atlas and all_instance_data are the main parts that need to be cached for reuse
    // Note: .prepare() is expected to populate this field.
    internal_data: Option<Arc<InternalTextLayerData>>,
}

impl TextLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: TextLayerParams,
    ) -> Self {
        // Error if point_radius_unit_mode is "data" when data_unit_mode is "pixels".
        if (layer_params.text_size_unit_mode == UnitsMode::Data && layer_params.data_unit_mode == UnitsMode::Pixels) {
            panic!("text_size_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        let data = Some(TextLayerData {
            // TODO: can cloning be avoided here?
            x_arr: layer_params.x_vec.clone(),
            y_arr: layer_params.y_vec.clone(),
            text_arr: layer_params.text_vec.clone(),
        });
        Self {
            view_params,
            layer_params,
            data,
            internal_data: None,
        }
    }
}




#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for TextLayer {
    async fn prepare(&mut self) {

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...

        // TODO: execute getters and cache the results.

        // For now, we use this function to prepare the font atlas and glyph cache.
        

        // TODO: in the future, we will need to extract the font atlas preparation logic to a base_ function to share with
        // descendant layers, so that they can asynchronously load their data in their prepare function
        // prior to their font atlas preparation.
        let data = self.data.as_ref().expect("Data was not provided for TextLayer.");

        let n = data.text_arr.len();
        let font_size = self.layer_params.text_size;
        let text_align_mode = self.layer_params.text_align_mode;
        let text_baseline_mode = self.layer_params.text_baseline_mode;

        // Build cache keys based on the data that affects the internal representation.
        // This includes: text strings, positions, font size, alignment, and baseline.
        let cache_keys: Vec<String> = vec![
            self.layer_params.layer_id.clone(),
            format!("{:?}", data.text_arr),
            format!("{:?}", data.x_arr),
            format!("{:?}", data.y_arr),
            format!("{}", font_size),
            format!("{:?}", text_align_mode),
            format!("{:?}", text_baseline_mode),
        ];

        // Use memoization to cache the internal data
        let internal_data = use_memo_internal_text_layer_data(async || {
            // Get cached font
            let font_atlas = get_or_init_font_atlas();

            // Build a comprehensive layout with all text elements to create the atlas
            let mut layout = Layout::new(CoordinateSystem::PositiveYUp);
            layout.reset(&LayoutSettings {
                max_width: None,
                max_height: None,
                ..LayoutSettings::default()
            });

            // Append all text from all elements to ensure we have all glyphs in the atlas
            for text_str in &data.text_arr {
                layout.append(
                    &[&font_atlas.font],
                    &TextStyle::new(&text_str, font_size as f32, 0),
                );
            }

            let glyphs = layout.glyphs();
            if glyphs.is_empty() {
                return InternalTextLayerData {
                    atlas_data: Vec::new(),
                    all_instance_data: Vec::new(),
                    atlas_width: 0,
                    atlas_height: 0,
                };
            }

            // Rasterize each glyph and measure atlas size (row pack)
            let mut atlas_width: usize = 0;
            let mut atlas_height: usize = 0;
            let mut rasters: Vec<(fontdue::Metrics, Vec<u8>)> = Vec::with_capacity(glyphs.len());

            for g in glyphs {
                // Rasterize the glyph to get its bitmap representation.
                let (metrics, bitmap) = font_atlas.font.rasterize_config(g.key);
                // Add padding around each glyph: PADDING + glyph_width + PADDING
                atlas_width += 2 * PADDING + metrics.width.max(1);
                atlas_height = atlas_height.max(2 * PADDING + metrics.height.max(1));
                rasters.push((metrics, bitmap));
            }

            if atlas_width == 0 || atlas_height == 0 {
                return InternalTextLayerData {
                    atlas_data: Vec::new(),
                    all_instance_data: Vec::new(),
                    atlas_width: 0,
                    atlas_height: 0,
                };
            }

            // Build the atlas RGBA (actually single channel) row - initialize with zeros for padding
            let mut atlas: Vec<u8> = vec![0u8; atlas_width * atlas_height];
            let mut x_cursor: usize = PADDING; // Start with padding offset

            // Now process each text element individually to generate instance data
            let mut all_instance_data: Vec<f32> = Vec::new();
            let mut total_instances = 0u32;

            // NOTE: atlas and all_instance_data are the main parts that need to be cached for reuse

            // Iterate over each string
            for elem_i in 0..n {
                let text_str = &data.text_arr[elem_i];
                let text_x_pos = data.x_arr[elem_i];
                let text_y_pos = data.y_arr[elem_i];

                // Measure text width for alignment.
                // Text width is in pixel units.
                let text_width = measure_text_width(
                    &font_atlas.font,
                    &text_str,
                    font_size as f32,
                );
                
                // Calculate offset based on alignment and baseline.
                // These offsets are in pixel units.
                let (offset_x, offset_y) = calculate_text_position(
                    font_size as f32,
                    text_align_mode,
                    text_baseline_mode,
                    text_width
                );

                // Create a separate layout for this text element
                let mut element_layout = Layout::new(CoordinateSystem::PositiveYUp);
                element_layout.reset(&LayoutSettings {
                    max_width: None,
                    max_height: None,
                    ..LayoutSettings::default()
                });
                element_layout.append(
                    &[&font_atlas.font],
                    &TextStyle::new(&text_str, font_size as f32, 0),
                );

                let element_glyphs = element_layout.glyphs();

                // Track our position in the atlas for this text element
                let mut element_cursor = x_cursor;

                // Iterate over each glyph in the string.
                for (i, g) in element_glyphs.iter().enumerate() {
                    let (m, bmp) = &rasters[total_instances as usize + i];

                    // Actual bitmap dimensions
                    let gw = m.width.max(0) as usize;
                    let gh = m.height.max(0) as usize;

                    // Copy bitmap into atlas with padding offset
                    if gw > 0 && gh > 0 {
                        for row in 0..gh {
                            let src = &bmp[row * gw..row * gw + gw];
                            // Offset destination by PADDING pixels vertically and horizontally
                            let dst_row = PADDING + row;
                            let dst_start = dst_row * atlas_width + element_cursor;
                            let dst_end = dst_start + gw;
                            let dst = &mut atlas[dst_start..dst_end];
                            dst.copy_from_slice(src);
                        }
                    }

                    // Compute screen-space rect for this glyph
                    // TODO: update this logic so that the rect is in whatever data_units_mode is?
                    // (ensure the text measurement is happening in the correct units too).
                    let x_px = offset_x + g.x as f32;
                    let y_px = offset_y + g.y as f32;
                    let w_px = g.width as f32;
                    let h_px: f32 = g.height as f32;

                    /*log(&format!("Glyph '{}' at (x={}, y={}), size (w={}, h={}), g.x={}, g.y={}, text_x_pos={}, text_y_pos={}, offset_x={}, offset_y={}",
                        elem_i, x_px, y_px, w_px, h_px, g.x, g.y, text_x_pos, text_y_pos, offset_x, offset_y
                    ));*/

                    // UV rectangle (normalized) - exclude padding from sampled area
                    let u0 = (element_cursor as f32) / (atlas_width as f32);
                    let v0 = (PADDING as f32) / (atlas_height as f32);
                    let u1 = ((element_cursor + gw) as f32) / (atlas_width as f32);
                    let v1 = ((PADDING + gh) as f32) / (atlas_height as f32);

                    if gw > 0 && gh > 0 {
                        all_instance_data.extend_from_slice(&[
                            text_x_pos, text_y_pos, // NOTE: these values can be in either data units or pixel units.
                            x_px, y_px, w_px, h_px, // NOTE: these values are always in pixel units.
                            u0, v0, u1, v1, // NOTE: these values are always indices into the atlas texture.
                        ]);
                    }

                    // Advance cursor by glyph width + padding for next glyph
                    element_cursor += gw + 2 * PADDING;
                }

                x_cursor = element_cursor;
                total_instances += element_glyphs.len() as u32;
            }

            // Return the internal data
            InternalTextLayerData {
                atlas_data: atlas,
                all_instance_data,
                atlas_width,
                atlas_height,
            }
        }, &cache_keys, self.view_params.cache_enabled).await;

        self.internal_data = Some(internal_data);
    }
}

// TODO: update this to allow for a color per text element.
#[derive(ShaderType, Debug)]
struct TextLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    data_unit_mode: u32, // 0 = pixels, 1 = data units
    text_size: f32,
    text_size_unit_mode: u32, // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    text_rotation: f32, // Rotation in degrees
    color: Vec4,
}

// We extract this function for reuse in derived scatterplot layers (e.g., ZarrTextLayer).
// TODO: is this the best way to share this logic?
// See https://www.youtube.com/watch?v=Phk0C-kLlho
// See https://github.com/linebender/xilem/blob/main/xilem_core/src/views/any_view.rs

// TODO: just pass view_params and layer_params here? But layer_params contains data too, which for some layers is not provided via constructor params...

pub async fn base_draw_text_layer(
    device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass<'_>,
    data: &TextLayerData,
    internal_data: &InternalTextLayerData,
    view_params: &ViewParams,
    layer_bounds: &Option<MarginParams>,
    data_unit_mode: &UnitsMode,
    text_size: f32,
    text_size_unit_mode: &UnitsMode,
    text_rotation: f32, // Rotation in degrees
) {
    // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
    let camera_view = view_params.camera_view.unwrap_or([
        // Column 0
        1.0, 0.0, 0.0, 0.0, // Column 1
        0.0, 1.0, 0.0, 0.0, // Column 2
        0.0, 0.0, 1.0, 0.0, // Column 3
        0.0, 0.0, 0.0, 1.0,
    ]);

    // Use layer-specific bounds if not None, otherwise use the view's margins
    // (which may also be None).
    let bounds = if layer_bounds.is_none() {
        &view_params.margins
    } else {
        layer_bounds
    };

    let margin_top = if let Some(margin_params) = &bounds {
        margin_params.margin_top.unwrap_or(0.0)
    } else { 0.0 } as f64;
    let margin_right = if let Some(margin_params) = &bounds {
        margin_params.margin_right.unwrap_or(0.0)
    } else { 0.0 } as f64;
    let margin_bottom = if let Some(margin_params) = &bounds {
        margin_params.margin_bottom.unwrap_or(0.0)
    } else { 0.0 } as f64;
    let margin_left = if let Some(margin_params) = &bounds {
        margin_params.margin_left.unwrap_or(0.0)
    } else { 0.0 } as f64;

    let viewport_w = view_params.width as f32;
    let viewport_h = view_params.height as f32;

    let layer_w = viewport_w - (margin_left + margin_right) as f32;
    let layer_h = viewport_h - (margin_top + margin_bottom) as f32;


    let atlas = &internal_data.atlas_data;
    let all_instance_data = &internal_data.all_instance_data;
    let atlas_width = internal_data.atlas_width;
    let atlas_height = internal_data.atlas_height;
    // Number of emitted instances (skip zero-sized glyphs)
    const NUM_VALUES_PER_INSTANCE: usize = 10;
    let instance_count: u32 = (all_instance_data.len() / NUM_VALUES_PER_INSTANCE) as u32;


    // Upload atlas as a single-channel R8Unorm texture
    let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Text Atlas"),
        size: wgpu::Extent3d {
            width: atlas_width as u32,
            height: atlas_height as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        atlas_tex.as_image_copy(),
        &atlas,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas_width as u32),
            rows_per_image: Some(atlas_height as u32),
        },
        wgpu::Extent3d {
            width: atlas_width as u32,
            height: atlas_height as u32,
            depth_or_array_layers: 1,
        },
    );

    let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Text Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });

    // 3) Create instance buffer
    let instance_buffer = device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Text Instances"),
            contents: bytemuck::cast_slice(&all_instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

    // Construct the uniform struct using Encase.
    let uniform_struct = TextLayerUniforms {
        layer_size: Vec2::new(layer_w, layer_h), // (layer_width, layer_height) in pixels
        camera_view: Mat4::from_cols_array(&camera_view),   // mat4x4<f32>,
        data_unit_mode: match data_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        }, // 0 = pixels, 1 = data units
        text_size: text_size,
        text_size_unit_mode: match text_size_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        }, // 0 = pixels, 1 = data units
        aspect_ratio_mode: match view_params.aspect_ratio_mode {
            AspectRatioMode::Ignore => 0,
            AspectRatioMode::Contain => 1,
            AspectRatioMode::Cover => 2,
        },
        aspect_ratio_alignment_mode: 0, // center. TODO
        text_rotation: text_rotation,
        // TODO: then, update the WGSL shader to match.
        // TODO: then, update the shader logic so that it does similar positioning logic
        // as done by the ScatterplotLayer vertex shader, using these uniform values.
        color: Vec4::from([0.0, 0.0, 0.0, 1.0]), // TODO: support per-element colors.
    };

    let mut buffer = UniformBuffer::new(Vec::<u8>::new());
    buffer.write(&uniform_struct).unwrap();
    let uniform_bytes = buffer.into_inner();

    let uniform_buffer = device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Text Uniforms"),
            contents: &uniform_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });

    // 5) Bind group layout: texture + sampler + uniforms
    let bind_group_layout = device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Text BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

    let bind_group = device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

    let shader = device
        .create_shader_module(wgpu::include_wgsl!("shaders/text_layer.wgsl"));

    let render_pipeline_layout = device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });
    
    // Vertex buffer layout: two vec4<f32> per instance
    let vertex_buffers = [wgpu::VertexBufferLayout {
        array_stride: (NUM_VALUES_PER_INSTANCE * std::mem::size_of::<f32>()) as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: (2 * std::mem::size_of::<f32>()) as u64,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: (6 * std::mem::size_of::<f32>()) as u64,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    }];

    let render_pipeline = device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    //blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });


    // Can everything before pass.set_pipeline be cached? Probably not the queue.write calls...

    // Handle margins by adjusting viewport and scissor rect.
    // This allows us to avoid accounting for margins in the shaders, simplifying them.
    // (Shaders can simply assume the full viewport size is the plot area.)
    // Note: these settings will affect all subsequent draw calls in this render pass,
    // so ensure that other layers are setting their own viewport/scissor_rect appropriately.

    // Set viewport so that the (-1 to 1) NDC coordinates map to the desired plot area within the canvas.
    pass.set_viewport(
        margin_left as f32,
        margin_top as f32,
        viewport_w - (margin_left + margin_right) as f32,
        viewport_h - (margin_top + margin_bottom) as f32,
        0.0, // min_depth
        1.0, // max_depth
    );

    // Set scissor rect so that fragments rendered into the margins are clipped.
    // "Sets the scissor rectangle used during the rasterization stage. After transformation into viewport coordinates."
    // "The function of the scissor rectangle resembles set_viewport(), but it does not affect the coordinate system, only which fragments are discarded."
    pass.set_scissor_rect(
        margin_left as u32,
        margin_top as u32,
        (viewport_w - (margin_left + margin_right) as f32) as u32,
        (viewport_h - (margin_top + margin_bottom) as f32) as u32,
    );

    pass.set_pipeline(&render_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.set_vertex_buffer(0, instance_buffer.slice(..));
    // 4 vertices (triangle strip) per instance
    pass.draw(0..4, 0..instance_count);

}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for TextLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");
        let internal_data = self.internal_data.as_ref().expect("Internal data was not prepared. Call prepare() first.");
        base_draw_text_layer(
            device, queue, pass,
            data,
            &internal_data,
            &self.view_params,
            &self.layer_params.bounds,
            &self.layer_params.data_unit_mode,
            self.layer_params.text_size,
            &self.layer_params.text_size_unit_mode,
            self.layer_params.text_rotation.unwrap_or(0.0),
        ).await;
    }
}

pub fn base_draw_text_layer_svg(
    data: &TextLayerData,
    view_params: &ViewParams,
    layer_bounds: &Option<MarginParams>,
    data_unit_mode: &UnitsMode,
    text_size: f32,
    text_size_unit_mode: &UnitsMode,
    text_align_mode: &TextAlignMode,
    text_baseline_mode: &TextBaselineMode,
    text_rotation: f32,
    layer_id: &str,
) -> Vec<TwoElement> {

    // Iterate over the data points and create SVG elements.
    let n = data.text_arr.len();

    // TODO: reduce code reuse here
    let camera_view = view_params.camera_view.unwrap_or([
        // Column 0
        1.0, 0.0, 0.0, 0.0, // Column 1
        0.0, 1.0, 0.0, 0.0, // Column 2
        0.0, 0.0, 1.0, 0.0, // Column 3
        0.0, 0.0, 0.0, 1.0,
    ]);

    // Use layer-specific bounds if not None, otherwise use the view's margins
    // (which may also be None).
    let bounds = if layer_bounds.is_none() {
        &view_params.margins
    } else {
        layer_bounds
    };

    let margin_top = if let Some(margin_params) = &bounds {
        margin_params.margin_top.unwrap_or(0.0)
    } else { 0.0 } as f64;
    let margin_right = if let Some(margin_params) = &bounds {
        margin_params.margin_right.unwrap_or(0.0)
    } else { 0.0 } as f64;
    let margin_bottom = if let Some(margin_params) = &bounds {
        margin_params.margin_bottom.unwrap_or(0.0)
    } else { 0.0 } as f64;
    let margin_left = if let Some(margin_params) = &bounds {
        margin_params.margin_left.unwrap_or(0.0)
    } else { 0.0 } as f64;

    let viewport_w = view_params.width as f32;
    let viewport_h = view_params.height as f32;

    let layer_w = viewport_w - (margin_left + margin_right) as f32;
    let layer_h = viewport_h - (margin_top + margin_bottom) as f32;
    // End TODO

    let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(n);
    for i in 0..n {
        let x = data.x_arr[i];
        let y = data.y_arr[i];

        // Convert data coordinates to pixel coordinates within the layer area.
        let (px, py) = get_point_position(
            x,
            y,
            layer_w,
            layer_h,
            &camera_view,
            *data_unit_mode,
            view_params.aspect_ratio_mode,
            0, // TODO: pass enum value for aspect_ratio_alignment_mode
        );

        // Create a circle or square element based on point_shape_mode.
        svg_elements.push(TwoElement::Text(TwoText {
            x: px as f64,
            y: py as f64,
            width: 100.0, // TODO?
            height: 100.0, // TODO?
            text: data.text_arr[i].clone(),
            font: "Arial".to_string(), // TODO: font should match the one used for raster rendering.
            fontsize: text_size as f64,
            // TODO: unify these enums.
            align: match text_align_mode {
                TextAlignMode::Start => TwoTextAlign::Start,
                TextAlignMode::Middle => TwoTextAlign::Middle,
                TextAlignMode::End => TwoTextAlign::End,
            },
            baseline: match text_baseline_mode {
                TextBaselineMode::Top => TwoTextBaseline::Top,
                TextBaselineMode::Middle => TwoTextBaseline::Middle,
                TextBaselineMode::Bottom => TwoTextBaseline::Bottom,
                TextBaselineMode::Alphabetic => TwoTextBaseline::Alphabetic,
            },
            rotation: Some(text_rotation as f64),
            // TODO: more params
            ..Default::default()
        }));
    }

    // Insert rects into an SVG group with a transform and clipping to handle margins,
    // similar to the usage of scissor rect and viewport in the Canvas rendering.
    let layer_group_vec = vec![
        TwoElement::Group(TwoGroup {
            elements: svg_elements,
            translate: Some((margin_left, margin_top)),
            layer_id: Some(layer_id.to_string()),
            // TODO: check how clip_rect interacts with the translate
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })
    ];

    return layer_group_vec;
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for TextLayer {
    async fn draw(&self, group: &Group) -> Group {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        let view_params = &self.view_params;
        let bounds = &self.layer_params.bounds;

        let svg_elements = base_draw_text_layer_svg(
            data,
            view_params,
            bounds,
            &self.layer_params.data_unit_mode,
            self.layer_params.text_size,
            &self.layer_params.text_size_unit_mode,
            &self.layer_params.text_align_mode,
            &self.layer_params.text_baseline_mode,
            self.layer_params.text_rotation.unwrap_or(0.0),
            &self.layer_params.layer_id,
        );
        
        // TODO: refactor to avoid the cloning here?
        let updated_group = update_svg(group.clone(), &svg_elements);

        return updated_group.clone();
        
    }
}
