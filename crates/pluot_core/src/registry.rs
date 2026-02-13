use crate::layer_traits::{PreparedAndDraw, ViewParams};

pub struct LayerRegistration {
    pub layer_type_name: &'static str,
    pub create_layer: fn(serde_json::Value, &ViewParams) -> Box<dyn PreparedAndDraw>,
}

inventory::collect!(LayerRegistration);

pub fn get_layer_from_registry(
    layer_type: &str,
    layer_params: serde_json::Value,
    view_params: &ViewParams,
) -> Box<dyn PreparedAndDraw> {
    for registration in inventory::iter::<LayerRegistration> {
        if registration.layer_type_name == layer_type {
            return (registration.create_layer)(layer_params, view_params);
        }
    }
    panic!("Unknown layer type: {}", layer_type);
}
