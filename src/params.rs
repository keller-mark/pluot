use crate::wgpu;
use crate::zarr::AsyncZarritaStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// TODO: use Observable Plot-like parameter names:
// https://observablehq.com/plot/marks/bar

#[derive(Serialize, Deserialize, Debug)]
pub struct ScatterplotRenderParams {
    pub x_key: String,
    pub y_key: String,
    pub color_key: Option<String>,
    pub point_radius: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Scatterplot3dRenderParams {
    pub x_key: String,
    pub y_key: String,
    pub z_key: String,
    pub color_key: Option<String>,
    pub point_radius: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BioimageRenderParams {
    pub channel_indices: Vec<u32>,
    pub channel_windows: Vec<(f32, f32)>,
    pub channel_colors: Vec<(f32, f32, f32)>, // RGB colors as floats in [0.0, 1.0]
    pub target_z: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BarPlotRenderParams {
    pub x_key: String,
    pub y_key: String,
    pub color_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "plot_type", content = "plot_params")]
pub enum PlotParams {
    // Using internally tagged enum representation.
    // { "plot_type": "Scatterplot" }
    // Reference: https://serde.rs/enum-representations.html
    Scatterplot(ScatterplotRenderParams),
    Scatterplot3d(Scatterplot3dRenderParams),
    Bioimage(BioimageRenderParams),
    BarPlot(BarPlotRenderParams),
    Triangle, // No parameters
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RenderParams {
    pub width: u32,
    pub height: u32,
    //pub zoom: Option<f32>,
    //pub target_x: Option<f32>,
    //pub target_y: Option<f32>,
    pub camera_view: Option<[f32; 16]>,

    #[serde(flatten)]
    pub plot_params: PlotParams,

    // We need a plot ID for cacheing of certain intermediate expensive computations per plot.
    // Note that solely data-dependent computations should be cached via the (store_name, key) tuple.
    pub plot_id: String,
    pub store_name: String,

    // Timeout in ms before bailing out of awaiting a data request.
    pub timeout: Option<u32>,

    // Margins for plots that need them (e.g. scatterplot axes).
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
}
pub struct RenderContext<'a> {
    pub store: &'a Arc<AsyncZarritaStore>,
    pub device: &'a wgpu::Device,
    pub texture_desc: &'a wgpu::TextureDescriptor<'a>,
    pub out_tex: &'a wgpu::Texture,
    pub queue: &'a wgpu::Queue,
    pub params: &'a RenderParams,

    pub vello_tex: &'a wgpu::Texture,
    //pub vello_scene: &'a mut vello::Scene,
}

pub struct RenderResult {
    pub bailed_early: bool,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            width: 100,
            height: 100,
            //zoom: None,
            //target_x: None,
            //target_y: None,
            camera_view: None,
            plot_id: "default_plot".to_string(),
            store_name: "default_store".to_string(),
            plot_params: PlotParams::Triangle,
            timeout: None,
            margin_left: None,
            margin_right: None,
            margin_top: None,
            margin_bottom: None,
        }
    }
}
