// Register layers from outside the core crate (e.g., from pluot_zarr).
use pluot_core::registry::LayerRegistration;
use pluot_zarr::layers::zarr_point_layer::{ZarrPointLayer, ZarrPointLayerParams};
use pluot_zarr::layers::zarr_point_3d_layer::{ZarrPoint3dLayer, ZarrPoint3dLayerParams};
use pluot_zarr::layers::zarr_bar_layer::{ZarrBarLayer, ZarrBarLayerParams};
use pluot_zarr::layers::ome_zarr_bitmap_layer::{OmeZarrBitmapLayer, OmeZarrBitmapLayerParams};
use pluot_zarr::layers::ome_zarr_multiscale_layer::{OmeZarrMultiscaleLayer, OmeZarrMultiscaleLayerParams};

// Ideally we could just run inventory::submit! in the pluot_zarr crate,
// but it is not working, so we do it here instead.
inventory::submit! {
    LayerRegistration {
        layer_type_name: "ZarrPointLayer",
        create_layer: |value, view_params| {
            let params: ZarrPointLayerParams = serde_json::from_value(value).unwrap();
            Box::new(ZarrPointLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "ZarrPoint3dLayer",
        create_layer: |value, view_params| {
            let params: ZarrPoint3dLayerParams = serde_json::from_value(value).unwrap();
            Box::new(ZarrPoint3dLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "ZarrBarLayer",
        create_layer: |value, view_params| {
            let params: ZarrBarLayerParams = serde_json::from_value(value).unwrap();
            Box::new(ZarrBarLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "OmeZarrBitmapLayer",
        create_layer: |value, view_params| {
            let params: OmeZarrBitmapLayerParams = serde_json::from_value(value).unwrap();
            Box::new(OmeZarrBitmapLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "OmeZarrMultiscaleLayer",
        create_layer: |value, view_params| {
            let params: OmeZarrMultiscaleLayerParams = serde_json::from_value(value).unwrap();
            Box::new(OmeZarrMultiscaleLayer::new(view_params.clone(), params))
        },
    }
}

// Note: Moving to the inventory-based registration system may have impacted performance a tiny bit.
// Consider just using crate features here to conditionally compile in the layers we want, instead of using inventory.
// But that would require moving more stuff to this crate, like the layered plot rendering code.
