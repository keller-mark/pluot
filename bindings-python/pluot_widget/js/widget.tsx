import React, { useMemo } from "react";
import { createRoot } from "react-dom/client";
import type { AnyModel, Render } from "@anywidget/types";
import { Pluot, setStoreByName } from "@pluot/react";
import {
  createKernelStore,
  createZarrGetter,
  useClickResult,
  useExtentResult,
  useModelBrush,
  useModelValue,
  useThrottledCameraMatrix,
  type WidgetModel,
} from "./hooks";

function PluotWidget({ model }: { model: AnyModel<WidgetModel> }) {
  const width = useModelValue(model, "width");
  const height = useModelValue(model, "height");
  const marginTop = useModelValue(model, "margin_top");
  const marginRight = useModelValue(model, "margin_right");
  const marginBottom = useModelValue(model, "margin_bottom");
  const marginLeft = useModelValue(model, "margin_left");
  const aspectRatioMode = useModelValue(model, "aspect_ratio_mode");
  const aspectRatioAlignmentMode = useModelValue(model, "aspect_ratio_alignment_mode");
  const viewMode = useModelValue(model, "view_mode");
  const plotId = useModelValue(model, "plot_id");
  const plotType = useModelValue(model, "plot_type");
  const storesMetadata = useModelValue(model, "stores_metadata");
  const plotParams = useModelValue(model, "plot_params");
  const format = useModelValue(model, "format");
  const minTimeout = useModelValue(model, "min_timeout");
  const maxTimeout = useModelValue(model, "max_timeout");
  const maxBailedEarlyRetries = useModelValue(model, "max_bailed_early_retries");
  const allowSimultaneousRenders = useModelValue(model, "allow_simultaneous_renders");
  const cameraFilter = useModelValue(model, "camera_filter");
  const debugMargins = useModelValue(model, "debug_margins");
  const backgroundColor = useModelValue(model, "background_color");
  const enableClick = useModelValue(model, "enable_click");
  const enableTooltip = useModelValue(model, "enable_tooltip");
  const enableBrushCreate = useModelValue(model, "enable_brush_create");
  const enableBrushEdit = useModelValue(model, "enable_brush_edit");
  const enableBrushClear = useModelValue(model, "enable_brush_clear");
  const brushMode = useModelValue(model, "brush_mode");
  const brushUnitsModeX = useModelValue(model, "brush_units_mode_x");
  const brushUnitsModeY = useModelValue(model, "brush_units_mode_y");
  const brushMarginTop = useModelValue(model, "brush_margin_top");
  const brushMarginRight = useModelValue(model, "brush_margin_right");
  const brushMarginBottom = useModelValue(model, "brush_margin_bottom");
  const brushMarginLeft = useModelValue(model, "brush_margin_left");
  const brushColor = useModelValue(model, "brush_color");
  const brushDelay = useModelValue(model, "brush_delay");
  const maybeBrushDelay = useModelValue(model, "maybe_brush_delay");

  const zarrGet = useMemo(() => createZarrGetter(model), [model]);

  // Registered during render rather than in an effect, so that the kernel-backed
  // stores exist before <Pluot/> issues its first read.
  useMemo(() => {
    for (const storeName of Object.keys(storesMetadata ?? {})) {
      setStoreByName(storeName, createKernelStore(zarrGet, storeName));
    }
  }, [zarrGet, storesMetadata]);

  const { cameraMatrix, setCameraMatrix, extentKey } = useThrottledCameraMatrix(model);
  const onExtent = useExtentResult(model);
  const onClick = useClickResult(model);
  const { brush, onBrush, onBrushEnd, onBrushClear } = useModelBrush(model);

  return (
    <Pluot
      width={width}
      height={height}
      plotId={plotId}
      plotType={plotType}
      plotParams={plotParams}
      stores={storesMetadata}
      registerStores={false}
      viewMode={viewMode}
      format={format}
      marginTop={marginTop}
      marginRight={marginRight}
      marginBottom={marginBottom}
      marginLeft={marginLeft}
      aspectRatioMode={aspectRatioMode}
      aspectRatioAlignmentMode={aspectRatioAlignmentMode}
      cameraMatrix={cameraMatrix}
      setCameraMatrix={setCameraMatrix}
      extentKey={extentKey}
      onExtent={onExtent}
      cameraFilter={cameraFilter}
      debugMargins={debugMargins}
      backgroundColor={backgroundColor ?? undefined}
      minTimeout={minTimeout}
      maxTimeout={maxTimeout}
      maxBailedEarlyRetries={maxBailedEarlyRetries}
      allowSimultaneousRenders={allowSimultaneousRenders}
      enableClick={enableClick}
      onClick={onClick}
      enableTooltip={enableTooltip}
      enableBrushCreate={enableBrushCreate}
      enableBrushEdit={enableBrushEdit}
      enableBrushClear={enableBrushClear}
      brushMode={brushMode}
      brushUnitsModeX={brushUnitsModeX}
      brushUnitsModeY={brushUnitsModeY}
      brushMarginTop={brushMarginTop ?? undefined}
      brushMarginRight={brushMarginRight ?? undefined}
      brushMarginBottom={brushMarginBottom ?? undefined}
      brushMarginLeft={brushMarginLeft ?? undefined}
      brushColor={brushColor}
      brushDelay={brushDelay}
      maybeBrushDelay={maybeBrushDelay}
      brush={brush}
      onBrush={onBrush}
      onBrushEnd={onBrushEnd}
      onBrushClear={onBrushClear}
    />
  );
}

const render: Render<WidgetModel> = ({ model, el }) => {
  const root = createRoot(el);
  root.render(<PluotWidget model={model} />);
  return () => root.unmount();
};

export default { render };
