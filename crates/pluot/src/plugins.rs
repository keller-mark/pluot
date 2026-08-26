// Register layers from outside the core crate (e.g., from pluot_zarr).
use pluot_core::registry::LayerRegistration;
use pluot_zarr::layers::zarr_point_layer::{ZarrPointLayer, ZarrPointLayerParams};
use pluot_zarr::layers::zarr_point_3d_layer::{ZarrPoint3dLayer, ZarrPoint3dLayerParams};
use pluot_zarr::layers::zarr_bar_plot_layer::{ZarrBarPlotLayer, ZarrBarPlotLayerParams};
use pluot_zarr::layers::zarr_histogram_layer::{ZarrHistogramLayer, ZarrHistogramLayerParams};
use pluot_zarr::layers::ome_zarr_bitmap_layer::{OmeZarrBitmapLayer, OmeZarrBitmapLayerParams};
use pluot_zarr::layers::ome_zarr_bitmap_multiscale_layer::{OmeZarrBitmapMultiscaleLayer, OmeZarrBitmapMultiscaleLayerParams};
use pluot_zarr::layers::ome_zarr_bitmask_layer::{OmeZarrBitmaskLayer, OmeZarrBitmaskLayerParams};
use pluot_zarr::layers::ome_zarr_bitmask_multiscale_layer::{OmeZarrBitmaskMultiscaleLayer, OmeZarrBitmaskMultiscaleLayerParams};
use pluot_zarr::layers::adata_zarr_dotplot_layer::{AdataZarrDotPlotLayer, AdataZarrDotPlotLayerParams};


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
        layer_type_name: "ZarrBarPlotLayer",
        create_layer: |value, view_params| {
            let params: ZarrBarPlotLayerParams = serde_json::from_value(value).unwrap();
            Box::new(ZarrBarPlotLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "ZarrHistogramLayer",
        create_layer: |value, view_params| {
            let params: ZarrHistogramLayerParams = serde_json::from_value(value).unwrap();
            Box::new(ZarrHistogramLayer::new(view_params.clone(), params))
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
        layer_type_name: "OmeZarrBitmapMultiscaleLayer",
        create_layer: |value, view_params| {
            let params: OmeZarrBitmapMultiscaleLayerParams = serde_json::from_value(value).unwrap();
            Box::new(OmeZarrBitmapMultiscaleLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "OmeZarrBitmaskLayer",
        create_layer: |value, view_params| {
            let params: OmeZarrBitmaskLayerParams = serde_json::from_value(value).unwrap();
            Box::new(OmeZarrBitmaskLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "OmeZarrBitmaskMultiscaleLayer",
        create_layer: |value, view_params| {
            let params: OmeZarrBitmaskMultiscaleLayerParams = serde_json::from_value(value).unwrap();
            Box::new(OmeZarrBitmaskMultiscaleLayer::new(view_params.clone(), params))
        },
    }
}

inventory::submit! {
    LayerRegistration {
        layer_type_name: "AdataZarrDotPlotLayer",
        create_layer: |value, view_params| {
            let params: AdataZarrDotPlotLayerParams = serde_json::from_value(value).unwrap();
            Box::new(AdataZarrDotPlotLayer::new(view_params.clone(), params))
        },
    }
}

// Consider just using crate features here to conditionally compile in the layers we want,
// instead of using inventory.
// But that would require moving more stuff to this crate, like the layered plot rendering code.
// Also see https://github.com/keller-mark/pluot/issues/178
