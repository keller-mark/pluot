use crate::wgpu;
use crate::zarr::AsyncZarritaStore;
use crate::render_traits::AspectRatioMode;
use serde::{Deserialize, Serialize};
use svg::node::element::Group;
use std::sync::Arc;


/// Select whether to use GPU or CPU for graphics rendering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RenderBackend {
    /// GPU via WebGPU render pipelines.
    Gpu,
    /// CPU
    Cpu,
}

/// Select whether to use GPU or CPU for compute operations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ComputeBackend {
    /// GPU via WebGPU compute pipelines.
    Gpu,
    /// CPU
    Cpu,
}

/// The graphics format for outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GraphicsFormat {
    /// Raster / Bitmap / Canvas / Pixels
    Raster,
    /// Vector / SVG
    Vector,

    // TODO: add AccessKit as a GraphicsFormat?
}

/// Whether displaying 2D versus 3D graphics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ViewMode {
    // 2D ~= OrthographicView in DeckGL terms
    // Reference: https://deck.gl/docs/developer-guide/views#types-of-views
    /// 2D
    #[serde(rename = "2d")]
    TwoD,
    // 3D ~= OrbitView in DeckGL terms
    /// 3D
    #[serde(rename = "3d")]
    ThreeD,
    // Note that 3D may have multiple camera modes
    // (e.g., orbit, turntable, matrix), but perhaps only the
    // interactive adapter needs to care about that.
    // Reference: https://github.com/mikolalysenko/3d-view
}
