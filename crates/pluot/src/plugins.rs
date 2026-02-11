// Register layers from outside the core crate (e.g., from pluot_zarr).
use pluot_core::registry::LayerRegistration;
use pluot_zarr::layers::zarr_scatterplot_layer::{ZarrScatterplotLayer, ZarrScatterplotLayerParams};

// Ideally we could just run inventory::submit! in the pluot_zarr crate,
// but it is not working, so we do it here instead.
inventory::submit! {
    LayerRegistration {
        layer_type_name: "ZarrScatterplotLayer",
        create_layer: |value, view_params| {
            let params: ZarrScatterplotLayerParams = serde_json::from_value(value).unwrap();
            Box::new(ZarrScatterplotLayer::new(view_params.clone(), params))
        },
    }
}

// Note: Moving to the inventory-based registration system may have impacted performance a tiny bit.
// Consider just using crate features here to conditionally compile in the layers we want, instead of using inventory.
