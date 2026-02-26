use crate::layer_traits::{MarginParams, PreparedAndDraw, ViewParams};
use crate::registry::get_layer_from_registry;
use crate::wgpu;
use crate::params::{PlotParams, LayerParams};
use crate::render_types::{RenderContext};

pub fn get_layer(layer_params: &LayerParams, view_params: &ViewParams) -> Box<dyn PreparedAndDraw> {
    get_layer_from_registry(&layer_params.layer_type, layer_params.layer_params.clone(), view_params)
}
