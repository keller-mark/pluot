import type { AnyModel, Render } from "@anywidget/types";
import { loadDeps, React, ReactDOMClient, type PluotReactModule } from "./deps";
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

function PluotWidget({ model, pluotReact }: { model: AnyModel<WidgetModel>; pluotReact: PluotReactModule }) {
  const { Pluot, setStoreByName } = pluotReact;
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

  const zarrGet = React.useMemo(() => createZarrGetter(model), [model]);

  // Registered during render rather than in an effect, so that the kernel-backed
  // stores exist before <Pluot/> issues its first read.
  React.useMemo(() => {
    for (const storeName of Object.keys(storesMetadata ?? {})) {
      setStoreByName(storeName, createKernelStore(zarrGet, storeName));
    }
  }, [zarrGet, storesMetadata]);

  const { cameraMatrix, setCameraMatrix, extentKey } = useThrottledCameraMatrix(model);
  const onExtent = useExtentResult(model);
  const onClick = useClickResult(model);
  const { brush, onBrush, onBrushEnd, onBrushClear } = useModelBrush(model);

  return React.createElement(Pluot, {
    width,
    height,
    plotId,
    plotType,
    plotParams,
    stores: storesMetadata,
    registerStores: false,
    viewMode,
    format,
    marginTop,
    marginRight,
    marginBottom,
    marginLeft,
    aspectRatioMode,
    aspectRatioAlignmentMode,
    cameraMatrix,
    setCameraMatrix,
    extentKey,
    onExtent,
    cameraFilter,
    debugMargins,
    backgroundColor: backgroundColor ?? undefined,
    minTimeout,
    maxTimeout,
    maxBailedEarlyRetries,
    allowSimultaneousRenders,
    enableClick,
    onClick,
    enableTooltip,
    enableBrushCreate,
    enableBrushEdit,
    enableBrushClear,
    brushMode,
    brushUnitsModeX,
    brushUnitsModeY,
    brushMarginTop: brushMarginTop ?? undefined,
    brushMarginRight: brushMarginRight ?? undefined,
    brushMarginBottom: brushMarginBottom ?? undefined,
    brushMarginLeft: brushMarginLeft ?? undefined,
    brushColor,
    brushDelay,
    maybeBrushDelay,
    brush,
    onBrush,
    onBrushEnd,
    onBrushClear,
  });
}

const render: Render<WidgetModel> = async ({ model, el }) => {
  const pluotReact = await loadDeps({
    jsDevMode: model.get("js_dev_mode"),
    jsPackageVersion: model.get("js_package_version"),
    customJsUrl: model.get("custom_js_url"),
  });
  const root = ReactDOMClient.createRoot(el);
  root.render(React.createElement(PluotWidget, { model, pluotReact }));
  return () => root.unmount();
};

export default { render };
