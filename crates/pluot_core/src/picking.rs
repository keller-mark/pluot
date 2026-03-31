use std::collections::HashMap;
use serde::Serialize;

use crate::wgpu;
use crate::wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages};
use crate::render_types::GpuContext;
use crate::params::{GraphicsFormat, RenderBackend, ComputeBackend};
use crate::render_traits::{MarginParams, PickableLayer, ViewParams, draw_layers_to_vector, draw_layers_to_raster};
use crate::cache::get_or_init_gpu_context;

use futures_intrusive::channel::shared::oneshot_channel;

use crate::viewport::{DataCoord, ScreenCoord, unproject};

#[derive(Serialize)]
pub struct LayerPickingResult {
    pub layer_id: String,
    pub info: HashMap<String, String>, // Additional info about the picked element (e.g., index in data array, value, etc.)
}

#[derive(Serialize)]
pub struct PickingResult {
    pub data_coord: Option<DataCoord>,
    pub screen_coord: ScreenCoord,
    pub layer_results: Vec<LayerPickingResult>,
}
