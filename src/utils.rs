use crate::zarr::{AsyncZarritaStore};
use std::sync::Arc;

// TODO: define RenderParams here (rather than lib.rs).
// Then, pass RenderParams via RenderContext.
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ScatterplotRenderParams {
    pub x_key: String,
    pub y_key: String,
    pub color_key: Option<String>,
    pub point_radius: Option<f32>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct BioimageRenderParams {
    pub channel_indices: Vec<u32>,
    pub channel_windows: Vec<(f32, f32)>,
    pub channel_colors: Vec<(f32, f32, f32)>, // RGB colors as floats in [0.0, 1.0]
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "plot_type", content = "plot_params")]
pub enum PlotParams {
    // Using internally tagged enum representation.
    // { "plot_type": "Scatterplot" }
    // Reference: https://serde.rs/enum-representations.html
    Scatterplot(ScatterplotRenderParams),
    Bioimage(BioimageRenderParams),
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
            plot_params: PlotParams::Scatterplot(ScatterplotRenderParams {
                x_key: "PC1".to_string(),
                y_key: "PC2".to_string(),
                color_key: None,
                point_radius: None,
            }),
        }
    }
}