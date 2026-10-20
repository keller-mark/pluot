//! [`HeatmapCsrLayer`] and [`HeatmapCscLayer`]: heatmaps of CSR or CSC sparse
//! matrices, which upload only the stored entries and look each cell up on
//! the GPU, rather than densifying the matrix.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::layers::heatmap_layer::{HeatmapColormap, HeatmapCommon, HeatmapValueBlock, MATRIX_VALUE_FIRST_BINDING};
use crate::numeric_data::NumericData;
use crate::picking::LayerPickingResult;
use crate::render_traits::{
    BrushableLayer, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, PickableLayer,
    PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use crate::sparse_matrix::{CompressedAxis, CompressedMatrix};
use crate::two::svg::SvgContext;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

/// Layer params struct for [`HeatmapCsrLayer`] and [`HeatmapCscLayer`].
/// Every field but `matrix` means the same as in
/// [`HeatmapLayerParams`](crate::layers::heatmap_layer::HeatmapLayerParams).
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct CompressedHeatmapLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,

    pub cell_offset: Option<(f32, f32)>,
    pub model_matrix: Option<[f32; 16]>,

    pub num_rows: u32,
    pub num_cols: u32,
    /// The `num_rows x num_cols` matrix, compressed along rows for a
    /// [`HeatmapCsrLayer`] or along columns for a [`HeatmapCscLayer`].
    pub matrix: CompressedMatrix,

    pub swap_axes: bool,

    pub colormap: Option<HeatmapColormap>,

    pub row_selection: Option<NumericData>,
    pub col_selection: Option<NumericData>,
    pub background_opacity: Option<f32>,

    pub opacity: f32,
}

pub type HeatmapCsrLayerParams = CompressedHeatmapLayerParams;
pub type HeatmapCscLayerParams = CompressedHeatmapLayerParams;

impl Default for CompressedHeatmapLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            cell_offset: None,
            model_matrix: None,
            num_rows: 0,
            num_cols: 0,
            matrix: CompressedMatrix::default(),
            swap_axes: false,
            colormap: None,
            row_selection: None,
            col_selection: None,
            background_opacity: None,
            opacity: 1.0,
        }
    }
}

impl CompressedHeatmapLayerParams {
    fn common(&self) -> HeatmapCommon<'_> {
        HeatmapCommon {
            layer_id: &self.layer_id,
            bounds: self.bounds.as_ref(),
            data_unit_mode_x: self.data_unit_mode_x,
            data_unit_mode_y: self.data_unit_mode_y,
            cell_offset: self.cell_offset,
            model_matrix: self.model_matrix,
            num_rows: self.num_rows,
            num_cols: self.num_cols,
            swap_axes: self.swap_axes,
            colormap: self.colormap.as_ref(),
            row_selection: self.row_selection.as_ref(),
            col_selection: self.col_selection.as_ref(),
            background_opacity: self.background_opacity,
            opacity: self.opacity,
        }
    }
}

/// The implementation shared by [`HeatmapCsrLayer`] and [`HeatmapCscLayer`].
struct CompressedHeatmapLayer {
    view_params: ViewParams,
    layer_params: CompressedHeatmapLayerParams,
    axis: CompressedAxis,
}

impl CompressedHeatmapLayer {
    fn new(view_params: ViewParams, layer_params: CompressedHeatmapLayerParams, axis: CompressedAxis) -> Self {
        layer_params.matrix.validate(axis, layer_params.num_rows as usize, layer_params.num_cols as usize);
        layer_params.common().validate();
        Self { view_params, layer_params, axis }
    }

    fn draw_gpu(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let layer_params = &self.layer_params;
        if layer_params.num_rows == 0 || layer_params.num_cols == 0 {
            return;
        }
        let prepared = layer_params.matrix.prepare_gpu(device, queue, self.axis, MATRIX_VALUE_FIRST_BINDING);
        let block = HeatmapValueBlock { first_row: 0, num_rows: layer_params.num_rows, textures: prepared.textures };
        layer_params.common().draw_gpu(&self.view_params, gpu_context, pass, &prepared.wgsl, &[block]);
    }

    async fn draw_svg(&self, ctx: &mut SvgContext) {
        let matrix = &self.layer_params.matrix;
        let value_at = |row: u32, col: u32| matrix.value_at(self.axis, row as usize, col as usize);
        self.layer_params.common().draw_svg(&self.view_params, ctx, value_at).await;
    }

    fn pick(&self, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let common = self.layer_params.common();
        let (row, col) = common.pick_cell(data_coord)?;
        let value = self.layer_params.matrix.format_value_at(self.axis, row as usize, col as usize);
        Some(common.picking_result(row, col, value))
    }
}

macro_rules! compressed_heatmap_layer {
    ($(#[$doc:meta])* $name:ident, $axis:expr) => {
        $(#[$doc])*
        pub struct $name(CompressedHeatmapLayer);

        impl $name {
            pub fn new(view_params: ViewParams, layer_params: CompressedHeatmapLayerParams) -> Self {
                Self(CompressedHeatmapLayer::new(view_params, layer_params, $axis))
            }
        }

        #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
        impl PreparedLayer for $name {
            async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
                PrepareResult { bailed_early: false }
            }
        }

        #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
        impl DrawToRasterGpu for $name {
            async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
                self.0.draw_gpu(gpu_context, pass);
            }
        }

        #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
        impl DrawToRasterCpu for $name {
            async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
        }

        #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
        impl DrawToSvg for $name {
            async fn draw(&self, ctx: &mut SvgContext) {
                self.0.draw_svg(ctx).await;
            }
        }

        impl BrushableLayer for $name {}

        impl ExtentableLayer for $name {}

        impl PickableLayer for $name {
            /// Returns the picked cell's "row", "col" and "value".
            fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
                self.0.pick(data_coord)
            }
        }

        inventory::submit! {
            crate::registry::LayerRegistration {
                layer_type_name: stringify!($name),
                create_layer: |value, view_params| {
                    let params: CompressedHeatmapLayerParams = serde_json::from_value(value).unwrap();
                    Box::new($name::new(view_params.clone(), params))
                },
            }
        }
    };
}

compressed_heatmap_layer!(
    /// A heatmap of a CSR matrix, whose `indptr` is indexed by row.
    HeatmapCsrLayer,
    CompressedAxis::Rows
);

compressed_heatmap_layer!(
    /// A heatmap of a CSC matrix, whose `indptr` is indexed by column.
    HeatmapCscLayer,
    CompressedAxis::Cols
);
