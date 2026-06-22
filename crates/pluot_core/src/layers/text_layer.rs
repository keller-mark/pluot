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

use crate::render_traits::{AspectRatioMode, AspectRatioAlignmentMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams, FontWeight, FontStyle};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::wgpu::util::DeviceExt; // This import enables usage of device.create_buffer_init
use crate::cache::{use_memo_internal_text_layer_data, CachedInternalTextLayerData};
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle,
    TwoColor, TwoText, TwoTextAlign, TwoTextBaseline
};
use crate::two::svg::{update_svg, SvgContext};
use crate::positioning::get_point_position;
use crate::log;
use crate::{zarr_get, zarr_get_status, FutureExt, Duration};
use crate::zarr_types::ZarrPeekResult;

const FONT_BYTES: &[u8] = include_bytes!("../../../../vendor/urw-core35-fonts/NimbusSans-Regular.ttf").as_slice();

#[cfg(feature = "embed_fonts")]
pub(crate) fn get_urw_font_bytes(font_family: &str, font_weight: FontWeight, font_style: FontStyle) -> Option<&'static [u8]> {
    match (font_family, font_weight, font_style) {
        ("Courier", FontWeight::Normal, FontStyle::Normal)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusMonoPS-Regular.ttf")),
        ("Courier", FontWeight::Bold, FontStyle::Normal)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusMonoPS-Bold.ttf")),
        ("Courier", FontWeight::Normal, FontStyle::Italic | FontStyle::Oblique)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusMonoPS-Italic.ttf")),
        ("Courier", FontWeight::Bold, FontStyle::Italic | FontStyle::Oblique)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusMonoPS-BoldItalic.ttf")),
        ("Helvetica", FontWeight::Normal, FontStyle::Normal)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusSans-Regular.ttf")),
        ("Helvetica", FontWeight::Bold, FontStyle::Normal)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusSans-Bold.ttf")),
        ("Helvetica", FontWeight::Normal, FontStyle::Italic | FontStyle::Oblique)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusSans-Oblique.ttf")),
        ("Helvetica", FontWeight::Bold, FontStyle::Italic | FontStyle::Oblique)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusSans-BoldOblique.ttf")),
        ("Times-Roman" | "Times", FontWeight::Normal, FontStyle::Normal)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusRoman-Regular.ttf")),
        ("Times-Roman" | "Times", FontWeight::Bold, FontStyle::Normal)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusRoman-Bold.ttf")),
        ("Times-Roman" | "Times", FontWeight::Normal, FontStyle::Italic | FontStyle::Oblique)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusRoman-Italic.ttf")),
        ("Times-Roman" | "Times", FontWeight::Bold, FontStyle::Italic | FontStyle::Oblique)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/NimbusRoman-BoldItalic.ttf")),
        ("Symbol", _, _)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/StandardSymbolsPS.ttf")),
        ("ZapfDingbats", _, _)
            => Some(include_bytes!("../../../../vendor/urw-core35-fonts/D050000L.ttf")),
        _ => None,
    }
}

#[cfg(not(feature = "embed_fonts"))]
pub(crate) fn get_urw_font_bytes(_font_family: &str, _font_weight: FontWeight, _font_style: FontStyle) -> Option<&'static [u8]> {
    None
}

// Cached font atlas data
#[derive(Clone)]
struct FontAtlasCache {
    font: Font,
    atlas_texture: Option<wgpu::Texture>,
    glyph_cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>, // char + font_size -> metrics + bitmap
}

// Cache key for the bundled default font.
const DEFAULT_FONT_CACHE_KEY: &str = "_default";

thread_local! {
    static FONT_ATLAS: RefCell<HashMap<String, FontAtlasCache>> = RefCell::new(HashMap::new());
}

fn get_or_init_font_atlas(font_bytes: &[u8], cache_key: &str) -> FontAtlasCache {
    FONT_ATLAS.with(|atlas| {
        let mut atlas_ref = atlas.borrow_mut();
        if let Some(cached_atlas) = atlas_ref.get(cache_key) {
            cached_atlas.clone()
        } else {
            let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("load font");
            let cache = FontAtlasCache {
                font,
                atlas_texture: None,
                glyph_cache: HashMap::new(),
            };
            atlas_ref.insert(cache_key.to_string(), cache.clone());
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

    // Use exact (non-ceil'd) advance widths to match SVG text width measurement.
    // fontdue internally uses ceil(advance_width) for cursor, causing slightly wider text.
    let mut exact_cursor = 0.0f32;
    let mut ceil_cursor = 0.0f32;
    for glyph in glyphs {
        let (metrics, _) = font.rasterize_config(glyph.key);
        let _ = glyph.x - ceil_cursor;
        ceil_cursor += metrics.advance_width.ceil();
        exact_cursor += metrics.advance_width;
    }
    exact_cursor
}

fn calculate_text_position(font_size: f32, text_align: TextAlignMode, text_baseline: TextBaselineMode, text_width: f32) -> (f32, f32) {
    let x = match text_align {
        TextAlignMode::Start => 0.0,
        TextAlignMode::Middle => 0.0 - text_width / 2.0,
        TextAlignMode::End => 0.0 - text_width,
    };

    let y = match text_baseline {
        TextBaselineMode::Top => font_size * 0.013,
        TextBaselineMode::Middle => font_size * 0.525,
        TextBaselineMode::Alphabetic => font_size * 0.785,
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

// Horizontal padding between glyphs to prevent left/right texture bleeding.
// No vertical padding: v=0 with ClampToEdge correctly samples the first row.
const H_PADDING: usize = 1;
const V_PADDING: usize = 1;
const RASTER_SCALE: f32 = 2.0; // Rasterize at 2x to improve quality at small sizes




// Build the cached internal data (font atlas + per-glyph instance data) for the given
// font bytes. The atlas is keyed by `cache_key` so its font is rasterized at most once.
// This is shared between the requested-font and fallback-font memoization paths.
fn build_internal_text_layer_data(
    font_bytes: &[u8],
    cache_key: &str,
    text_vec: &[String],
    position_x: &[f32],
    position_y: &[f32],
    font_size: f32,
    text_align_mode: TextAlignMode,
    text_baseline_mode: TextBaselineMode,
) -> CachedInternalTextLayerData {
    let n = text_vec.len();
    let raster_font_size = font_size * RASTER_SCALE;

    // Get cached font atlas for these bytes.
    let font_atlas = get_or_init_font_atlas(font_bytes, cache_key);

    // Baseline position in raster space: -ceil(ascent). Matches fontdue's internal
    // baseline_y so we can use exact m.bounds.ymin instead of floor'd g.y.
    let baseline_y_raster: f32 = font_atlas.font
        .horizontal_line_metrics(raster_font_size)
        .map_or(-raster_font_size, |lm| -lm.ascent.ceil());

    // Build a comprehensive layout with all text elements to create the atlas
    let mut layout = Layout::new(CoordinateSystem::PositiveYUp);
    layout.reset(&LayoutSettings {
        max_width: None,
        max_height: None,
        ..LayoutSettings::default()
    });

    // Append all text from all elements to ensure we have all glyphs in the atlas
    for text_str in text_vec.iter() {
        layout.append(
            &[&font_atlas.font],
            &TextStyle::new(text_str, raster_font_size, 0),
        );
    }

    let glyphs = layout.glyphs();
    if glyphs.is_empty() {
        return CachedInternalTextLayerData {
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
        let w1 = g.width;
        let h1 = g.height;
        let (metrics, bitmap) = font_atlas.font.rasterize_config(g.key);
        atlas_width += 2 * H_PADDING + metrics.width.max(1);
        atlas_height = atlas_height.max(2 * V_PADDING + metrics.height.max(1));
        rasters.push((metrics, bitmap));
    }

    if atlas_width == 0 || atlas_height == 0 {
        return CachedInternalTextLayerData {
            atlas_data: Vec::new(),
            all_instance_data: Vec::new(),
            atlas_width: 0,
            atlas_height: 0,
        };
    }

    // Build the atlas RGBA (actually single channel) row - initialize with zeros for padding
    let mut atlas: Vec<u8> = vec![0u8; atlas_width * atlas_height];
    let mut x_cursor: usize = 0; // First glyph starts at 0; ClampToEdge handles the left boundary

    // Now process each text element individually to generate instance data
    let mut all_instance_data: Vec<f32> = Vec::new();
    let mut total_instances = 0u32;

    // NOTE: atlas and all_instance_data are the main parts that need to be cached for reuse

    // Iterate over each string
    for elem_i in 0..n {
        let text_str = &text_vec[elem_i];
        let text_x_pos = position_x[elem_i];
        let text_y_pos = position_y[elem_i];

        // Measure text width for alignment.
        // Text width is in pixel units.
        let text_width = measure_text_width(
            &font_atlas.font,
            text_str,
            raster_font_size,
        ) / RASTER_SCALE;

        // Calculate offset based on alignment and baseline.
        // These offsets are in pixel units.
        let (offset_x, offset_y) = calculate_text_position(
            font_size,
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
            &TextStyle::new(text_str, raster_font_size, 0),
        );

        let element_glyphs = element_layout.glyphs();

        // Track our position in the atlas for this text element
        let mut element_cursor = x_cursor;

        // Precompute exact x-positions for all glyphs in this element.
        // fontdue's layout uses ceil(advance_width) for cursor advancement,
        // which causes character spacing to be slightly wider than SVG's.
        // We recompute using exact (fractional) advance widths to match SVG.
        let exact_glyph_x: Vec<f32> = {
            let mut ceil_cursor = 0.0f32;
            let mut exact_cursor = 0.0f32;
            element_glyphs.iter().enumerate().map(|(i, g)| {
                let m = &rasters[total_instances as usize + i].0;
                let xmin = g.x - ceil_cursor;
                let ex = exact_cursor + xmin;
                ceil_cursor += m.advance_width.ceil();
                exact_cursor += m.advance_width;
                ex
            }).collect()
        };

        // Iterate over each glyph in the string.
        for (i, g) in element_glyphs.iter().enumerate() {
            let (m, bmp) = &rasters[total_instances as usize + i];

            // Actual bitmap dimensions
            let gw = m.width.max(0);
            let gh = m.height.max(0);

            // Copy bitmap into atlas (no vertical padding — v=0/ClampToEdge handles top edge)
            if gw > 0 && gh > 0 {
                for row in 0..gh {
                    let src = &bmp[row * gw..row * gw + gw];
                    let dst_start = (V_PADDING + row) * atlas_width + element_cursor;
                    atlas[dst_start..dst_start + gw].copy_from_slice(src);
                }
            }

            // Compute screen-space rect for this glyph using exact advance-based x.
            // Divide by RASTER_SCALE to convert from 2x raster space back to 1x screen pixels.
            // Use m.bounds.ymin (exact, non-floor'd) instead of g.y (which bakes in
            // fontdue's floor(ymin) rounding) so all baseline-sitting glyphs share the
            // same y_px regardless of sub-pixel ymin variation.
            let x_px = offset_x + exact_glyph_x[i] / RASTER_SCALE;
            let y_px = offset_y + (m.bounds.ymin + baseline_y_raster) / RASTER_SCALE;
            let w_px = g.width as f32 / RASTER_SCALE;
            let h_px: f32 = g.height as f32 / RASTER_SCALE;

            // UV: no vertical padding, relies on ClampToEdge at v=0 for correct top-edge sampling
            let u0 = (element_cursor as f32) / (atlas_width as f32);
            let v0 = (V_PADDING as f32) / (atlas_height as f32);
            let u1 = ((element_cursor + gw) as f32) / (atlas_width as f32);
            let v1 = ((V_PADDING + gh) as f32) / (atlas_height as f32);

            if gw > 0 && gh > 0 {
                all_instance_data.extend_from_slice(&[
                    text_x_pos, text_y_pos, // NOTE: these values can be in either data units or pixel units.
                    x_px, y_px, w_px, h_px, // NOTE: these values are always in pixel units.
                    u0, v0, u1, v1, // NOTE: these values are always indices into the atlas texture.
                ]);
            }

            // Advance cursor by glyph width + padding for next glyph
            element_cursor += gw + 2 * H_PADDING;
        }

        x_cursor = element_cursor;
        total_instances += element_glyphs.len() as u32;
    }

    // Return the internal data
    CachedInternalTextLayerData {
        atlas_data: atlas,
        all_instance_data,
        atlas_width,
        atlas_height,
    }
}

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
#[serde(default)]
pub struct TextLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode, // Units of x/y positions.
    pub data_unit_mode_y: UnitsMode, // Units of x/y positions.
    pub text_size: f32,
    pub text_size_unit_mode: UnitsMode, // Units of the font size.
    pub text_align_mode: TextAlignMode,
    pub text_baseline_mode: TextBaselineMode,
    pub text_rotation: Option<f32>, // Rotation in degrees
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix
    // Optional font name to request from the client environment.
    // If None or if the request fails, the bundled default font is used.
    pub font_family: Option<String>,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,

    pub position_x: Arc<Vec<f32>>, // TODO: generalize to other numeric dtypes?
    pub position_y: Arc<Vec<f32>>,
    pub text_vec: Arc<Vec<String>>,
}

impl Default for TextLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            text_size: 12.0,
            text_size_unit_mode: UnitsMode::Pixels,
            text_align_mode: TextAlignMode::Start,
            text_baseline_mode: TextBaselineMode::Alphabetic,
            text_rotation: None,
            model_matrix: None,
            font_family: None,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            position_x: Arc::new(vec![]),
            position_y: Arc::new(vec![]),
            text_vec: Arc::new(vec![]),
        }
    }
}

// TODO: defaults for params?

// Re-export the cached internal data type for convenience.
pub type InternalTextLayerData = CachedInternalTextLayerData;

pub struct TextLayer {
    view_params: ViewParams,
    layer_params: TextLayerParams,

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
        if layer_params.text_size_unit_mode == UnitsMode::Data && (layer_params.data_unit_mode_x == UnitsMode::Pixels || layer_params.data_unit_mode_y == UnitsMode::Pixels) {
            panic!("text_size_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        Self {
            view_params,
            layer_params,
            internal_data: None,
        }
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for TextLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...

        // TODO: execute getters and cache the results.

        // For now, we use this function to prepare the font atlas and glyph cache.


        // TODO: in the future, we will need to extract the font atlas preparation logic to a base_ function to share with
        // descendant layers, so that they can asynchronously load their data in their prepare function
        // prior to their font atlas preparation.

        let font_size = self.layer_params.text_size;
        let text_align_mode = self.layer_params.text_align_mode;
        let text_baseline_mode = self.layer_params.text_baseline_mode;
        let cache_enabled = self.view_params.cache_enabled;

        let weight_str = match self.layer_params.font_weight {
            FontWeight::Normal => "Normal",
            FontWeight::Bold => "Bold",
        };
        let style_str = match self.layer_params.font_style {
            FontStyle::Normal => "Normal",
            FontStyle::Italic => "Italic",
            FontStyle::Oblique => "Oblique",
        };

        // Base cache keys shared by both the requested-font and fallback-font memoizations.
        // The font-specific key is appended per step so each font caches under its own entry.
        // This includes: layer id, text strings, positions, font size, alignment, baseline.
        let base_cache_keys: Vec<String> = vec![
            self.layer_params.layer_id.clone(),
            format!("{:?}", self.layer_params.text_vec), // TODO: use a better key
            format!("{:?}", self.layer_params.position_x), // TODO: use a better key
            format!("{:?}", self.layer_params.position_y), // TODO: use a better key
            format!("{}", font_size),
            format!("{:?}", text_align_mode),
            format!("{:?}", text_baseline_mode),
        ];

        // Step 1: attempt to load and cache the internal data for the requested font.
        // Both the zarr_get_status and zarr_get calls live inside the memoization callback
        // so they only run on a cache miss. The callback short-circuits with None *only*
        // when the font is still loading (Pending): nothing is cached under the requested
        // key, and we fall through to the bundled default font below for this render. Once
        // the font arrives, the cache miss fires again and the data is computed and cached.
        //
        // For a rejected or timed-out font we instead build the fallback font cached under
        // the requested key and return Some: such a font will not arrive later, so there is
        // no point short-circuiting (which would also signal a wasteful re-prepare). Thus
        // None unambiguously means "the requested font is pending".
        let requested_internal_data = if let Some(ref font_family) = self.layer_params.font_family {
            let requested_font_key = format!("{}/{}/{}", font_family, style_str, weight_str);
            let mut requested_cache_keys = base_cache_keys.clone();
            requested_cache_keys.push(requested_font_key.clone());

            use_memo_internal_text_layer_data(async || {
                // Resolve the requested font's bytes, either from the embedded URW fonts
                // (synchronous) or from the __fonts__ zarr store (async). None here means
                // we will fall back to the bundled default font under the requested key.
                let custom_font_bytes: Option<Vec<u8>> = if let Some(bytes) = get_urw_font_bytes(
                    font_family,
                    self.layer_params.font_weight,
                    self.layer_params.font_style,
                ) {
                    Some(bytes.to_vec())
                } else {
                    let font_key = format!("{}/{}/{}.ttf", font_family, style_str, weight_str);
                    match zarr_get_status("__fonts__", &font_key) {
                        // Font not yet available — short-circuit without caching so the
                        // fallback font is used this render and we re-prepare later.
                        ZarrPeekResult::Pending => return None,
                        ZarrPeekResult::Fulfilled => {
                            let font_future = zarr_get("__fonts__", &font_key);
                            match crate::maybe_timeout!(font_future, self.view_params.timeout).await {
                                Ok(bytes) => Some(bytes.to_vec()),
                                Err(_) => None, // Timeout — fall back to the bundled default font.
                            }
                        }
                        ZarrPeekResult::Rejected => None, // Fall back to the bundled default font.
                    }
                };
                let font_bytes: &[u8] = custom_font_bytes.as_deref().unwrap_or(FONT_BYTES);
                Some(build_internal_text_layer_data(
                    font_bytes,
                    &requested_font_key,
                    &self.layer_params.text_vec,
                    &self.layer_params.position_x,
                    &self.layer_params.position_y,
                    font_size,
                    text_align_mode,
                    text_baseline_mode,
                ))
            }, &requested_cache_keys, cache_enabled).await
        } else {
            None
        };

        // The requested font is still loading iff a font was requested and step 1
        // short-circuited (returned None). Signal the caller to re-prepare once it arrives.
        let font_pending = self.layer_params.font_family.is_some() && requested_internal_data.is_none();

        // Step 2: if the requested font was unavailable (not requested, or still pending),
        // build and cache the bundled default font under its own key. The fallback is
        // cached too so we don't recompute its atlas each render.
        let internal_data = match requested_internal_data {
            Some(data) => data,
            None => {
                let mut fallback_cache_keys = base_cache_keys;
                fallback_cache_keys.push(DEFAULT_FONT_CACHE_KEY.to_string());

                use_memo_internal_text_layer_data(async || {
                    Some(build_internal_text_layer_data(
                        FONT_BYTES,
                        DEFAULT_FONT_CACHE_KEY,
                        &self.layer_params.text_vec,
                        &self.layer_params.position_x,
                        &self.layer_params.position_y,
                        font_size,
                        text_align_mode,
                        text_baseline_mode,
                    ))
                }, &fallback_cache_keys, cache_enabled)
                .await
                .expect("fallback font always produces internal data")
            }
        };

        self.internal_data = Some(internal_data);

        return PrepareResult {
            bailed_early: font_pending,
        }
    }
}

// TODO: update this to allow for a color per text element.
#[derive(ShaderType, Debug)]
struct TextLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    data_unit_mode_x: u32, // 0 = pixels, 1 = data units
    data_unit_mode_y: u32, // 0 = pixels, 1 = data units
    text_size: f32,
    text_size_unit_mode: u32, // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    model_matrix: Mat4, // mat4x4<f32> for affine transformations of the image.
    text_rotation: f32, // Rotation in degrees
    color: Vec4,
}

// We extract this function for reuse in derived scatterplot layers (e.g., ZarrTextLayer).
// TODO: is this the best way to share this logic?
// See https://www.youtube.com/watch?v=Phk0C-kLlho
// See https://github.com/linebender/xilem/blob/main/xilem_core/src/views/any_view.rs

// TODO: just pass view_params and layer_params here? But layer_params contains data too, which for some layers is not provided via constructor params...

pub async fn base_draw_text_layer(
    gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass<'_>,
    view_params: &ViewParams,
    layer_params: &TextLayerParams,
    internal_data: &InternalTextLayerData,
) {
    let GpuContext { device, queue } = gpu_context;
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
    let bounds = if layer_params.bounds.is_none() {
        &view_params.margins
    } else {
        &layer_params.bounds
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
        atlas,
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
            contents: bytemuck::cast_slice(all_instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

    // Construct the uniform struct using Encase.
    let uniform_struct = TextLayerUniforms {
        layer_size: Vec2::new(layer_w, layer_h), // (layer_width, layer_height) in pixels
        camera_view: Mat4::from_cols_array(&camera_view),   // mat4x4<f32>,
        data_unit_mode_x: match layer_params.data_unit_mode_x {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        },
        data_unit_mode_y: match layer_params.data_unit_mode_y {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        },
        text_size: layer_params.text_size,
        text_size_unit_mode: match layer_params.text_size_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        }, // 0 = pixels, 1 = data units
        aspect_ratio_mode: match view_params.aspect_ratio_mode {
            AspectRatioMode::Ignore => 0,
            AspectRatioMode::Contain => 1,
            AspectRatioMode::Cover => 2,
        },
        aspect_ratio_alignment_mode: match view_params.aspect_ratio_alignment_mode {
            AspectRatioAlignmentMode::Center => 0,
            AspectRatioAlignmentMode::Start => 1,
            AspectRatioAlignmentMode::End => 2,
        },
        model_matrix: Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
            // Column 0
            1.0, 0.0, 0.0, 0.0, // Column 1
            0.0, 1.0, 0.0, 0.0, // Column 2
            0.0, 0.0, 1.0, 0.0, // Column 3
            0.0, 0.0, 0.0, 1.0,
        ])),
        text_rotation: layer_params.text_rotation.unwrap_or(0.0),
        // TODO: then, update the WGSL shader to match.
        // TODO: then, update the shader logic so that it does similar positioning logic
        // as done by the PointLayer vertex shader, using these uniform values.
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
            bind_group_layouts: &[Some(&bind_group_layout)],
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
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    //blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        // Composite alpha correctly: a_out = a_src + (1 - a_src) * a_dst.
                        // One/Zero would replace dst alpha with src alpha, erasing
                        // alpha from layers rendered below the text layer.
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
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
impl DrawToRasterGpu for TextLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let internal_data = self.internal_data.as_ref().expect("Internal data was not prepared. Call prepare() first.");
        base_draw_text_layer(
            gpu_context, pass,
            &self.view_params,
            &self.layer_params,
            internal_data,
        ).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for TextLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

pub fn base_draw_text_layer_svg(
    view_params: &ViewParams,
    layer_params: &TextLayerParams,
) -> Vec<TwoElement> {

    // Iterate over the data points and create SVG elements.
    let n = layer_params.text_vec.len();

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
    let bounds = if layer_params.bounds.is_none() {
        &view_params.margins
    } else {
        &layer_params.bounds
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

    let model_matrix_raw: [f32; 16] = layer_params.model_matrix.unwrap_or([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]);
    // End TODO

    let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(n);
    for i in 0..n {
        let x = layer_params.position_x[i];
        let y = layer_params.position_y[i];

        // Convert data coordinates to pixel coordinates within the layer area.
        let (px, py) = get_point_position(
            x,
            y,
            layer_w,
            layer_h,
            &camera_view,
            layer_params.data_unit_mode_x,
            layer_params.data_unit_mode_y,
            view_params.aspect_ratio_mode,
            view_params.aspect_ratio_alignment_mode,
            Some(&model_matrix_raw),
        );

        // Create a circle or square element based on point_shape_mode.
        svg_elements.push(TwoElement::Text(TwoText {
            x: px as f64,
            y: (layer_h - py) as f64,
            text: layer_params.text_vec[i].clone(),
            font_family: layer_params.font_family.clone().unwrap_or_else(|| "Helvetica".to_string()),
            font_weight: match layer_params.font_weight {
                FontWeight::Normal => "normal".to_string(),
                FontWeight::Bold => "bold".to_string(),
            },
            font_style: match layer_params.font_style {
                FontStyle::Normal => "normal".to_string(),
                FontStyle::Italic => "italic".to_string(),
                FontStyle::Oblique => "oblique".to_string(),
            },
            fontsize: layer_params.text_size as f64,
            // TODO: unify these enums.
            align: match layer_params.text_align_mode {
                TextAlignMode::Start => TwoTextAlign::Start,
                TextAlignMode::Middle => TwoTextAlign::Middle,
                TextAlignMode::End => TwoTextAlign::End,
            },
            baseline: match layer_params.text_baseline_mode {
                TextBaselineMode::Top => TwoTextBaseline::Top,
                TextBaselineMode::Middle => TwoTextBaseline::Middle,
                TextBaselineMode::Bottom => TwoTextBaseline::Bottom,
                TextBaselineMode::Alphabetic => TwoTextBaseline::Alphabetic,
            },
            rotation: Some(layer_params.text_rotation.unwrap_or(0.0) as f64),
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
            layer_id: Some(layer_params.layer_id.clone()),
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
    async fn draw(&self, ctx: &mut SvgContext) {
        let svg_elements = base_draw_text_layer_svg(
            &self.view_params,
            &self.layer_params,
        );
        update_svg(ctx, &svg_elements);
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "TextLayer",
        create_layer: |value, view_params| {
            let params: TextLayerParams = serde_json::from_value(value).unwrap();
            Box::new(TextLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for TextLayer {}
