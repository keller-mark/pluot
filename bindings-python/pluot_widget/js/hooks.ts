import type { AnyModel } from "@anywidget/types";
import * as uuid from "@lukeed/uuid";
import type {
  setStoreByName,
  AspectRatioAlignmentMode,
  AspectRatioMode,
  BrushingResult,
  BrushMode,
  BrushResult,
  BrushState,
  BrushUnitsMode,
  CameraMatrix,
  CameraOrExtent,
  ExtentResult,
  GraphicsFormat,
  NoBrush,
  PickingResult,
  PlotParams,
  PlotType,
  PluotProps,
  StoresInput,
  ViewMode,
} from "@pluot/react";
import { React } from "./deps";

// @pluot/react is only loaded at render time, since its version and URL are widget options.
const NO_BRUSH: NoBrush = "NoBrush";

type AsyncReadable = Parameters<typeof setStoreByName>[1];
type RangeQuery = Parameters<NonNullable<AsyncReadable["getRange"]>>[1];

type ZarrGetParams = [storeName: string, key: string] | [storeName: string, key: string, rangeQuery: RangeQuery];
type CommandResponse = { success: boolean };

export type WidgetModel = {
  width: number;
  height: number;
  camera_view: number[] | null;
  x_lim: [number, number] | null;
  y_lim: [number, number] | null;
  extent: ExtentResult | null;
  margin_top: number;
  margin_right: number;
  margin_bottom: number;
  margin_left: number;
  aspect_ratio_mode: AspectRatioMode;
  aspect_ratio_alignment_mode: AspectRatioAlignmentMode;
  view_mode: ViewMode;
  camera_filter: Extract<PluotProps["cameraFilter"], string> | null;
  debug_margins: boolean;
  background_color: string | null;
  enable_click: boolean;
  enable_tooltip: boolean;
  click_result: PickingResult | null;
  enable_brush_create: boolean;
  enable_brush_edit: boolean;
  enable_brush_clear: boolean;
  brush_mode: BrushMode;
  brush_units_mode_x: BrushUnitsMode;
  brush_units_mode_y: BrushUnitsMode;
  brush_margin_top: number | null;
  brush_margin_right: number | null;
  brush_margin_bottom: number | null;
  brush_margin_left: number | null;
  brush_color: string;
  // Milliseconds.
  brush_delay: number;
  // Milliseconds.
  maybe_brush_delay: number;
  brush: BrushState | null;
  brushing_result: BrushingResult | null;
  plot_id: string;
  plot_type: PlotType;
  stores_metadata: StoresInput;
  plot_params: PlotParams;
  format: GraphicsFormat;
  batch_zarr_gets: boolean;
  // Milliseconds.
  min_timeout: number;
  // Milliseconds.
  max_timeout: number;
  max_bailed_early_retries: number;
  allow_simultaneous_renders: boolean;
  // Milliseconds.
  camera_sync_interval: number;
  js_dev_mode: boolean;
  js_package_version: string;
  custom_js_url: string;
};

type PendingGet = {
  params: ZarrGetParams;
  resolve: (value: Uint8Array | undefined) => void;
  reject: (reason: unknown) => void;
};

// Custom invoke matching the anywidget-command protocol implemented on the Python side.
function invoke<T>(
  model: AnyModel<WidgetModel>,
  name: string,
  msg: unknown,
  signal: AbortSignal = AbortSignal.timeout(30000),
): Promise<[T, DataView[]]> {
  // crypto.randomUUID is unavailable in non-secure (http://) contexts.
  const id = uuid.v4();
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(signal.reason);
      return;
    }
    function handler(responseMsg: any, responseBuffers: DataView[]) {
      if (!responseMsg || responseMsg.id !== id) return;
      model.off("msg:custom", handler);
      resolve([responseMsg.response, responseBuffers]);
    }
    signal.addEventListener("abort", () => {
      model.off("msg:custom", handler);
      reject(signal.reason);
    });
    model.on("msg:custom", handler);
    model.send({ id, kind: "anywidget-command", name, msg }, undefined, []);
  });
}

function bufferFromResponse(bufferData: DataView | ArrayBuffer): Uint8Array {
  if (ArrayBuffer.isView(bufferData)) {
    return new Uint8Array(bufferData.buffer, bufferData.byteOffset, bufferData.byteLength);
  }
  return new Uint8Array(bufferData);
}

export function createZarrGetter(model: AnyModel<WidgetModel>) {
  let pending: PendingGet[] = [];
  let batchId = 0;

  async function processBatch(batch: PendingGet[]) {
    try {
      const [dataArr, buffersArr] = await invoke<CommandResponse[]>(
        model,
        "_zarr_get_multi",
        batch.map(d => d.params),
      );
      batch.forEach((item, i) => {
        item.resolve(dataArr[i].success ? bufferFromResponse(buffersArr[i]) : undefined);
      });
    } catch (err) {
      batch.forEach(item => item.reject(err));
    }
  }

  function flush() {
    const batch = pending;
    pending = [];
    batchId = 0;
    processBatch(batch);
  }

  function enqueue(params: ZarrGetParams): Promise<Uint8Array | undefined> {
    batchId = batchId || requestAnimationFrame(flush);
    const { promise, resolve, reject } = Promise.withResolvers<Uint8Array | undefined>();
    pending.push({ params, resolve, reject });
    return promise;
  }

  async function getOne(params: ZarrGetParams): Promise<Uint8Array | undefined> {
    const name = params.length === 2 ? "_zarr_get" : "_zarr_get_range";
    const [data, buffers] = await invoke<CommandResponse>(model, name, params);
    return data.success ? bufferFromResponse(buffers[0]) : undefined;
  }

  return (params: ZarrGetParams) => model.get("batch_zarr_gets") ? enqueue(params) : getOne(params);
}

// AsyncReadable store that proxies all reads back to the Python kernel.
export function createKernelStore(zarrGet: ReturnType<typeof createZarrGetter>, storeName: string): AsyncReadable {
  return {
    get: (key) => {
      return zarrGet([storeName, key]);
    },
    getRange: (key, rangeQuery) => {
      return zarrGet([storeName, key, rangeQuery])
    },
  };
}

export function useModelValue<K extends keyof WidgetModel>(model: AnyModel<WidgetModel>, key: K): WidgetModel[K] {
  const subscribe = React.useCallback((onChange: () => void) => {
    model.on(`change:${key}`, onChange);
    return () => model.off(`change:${key}`, onChange);
  }, [model, key]);
  return React.useSyncExternalStore(subscribe, () => model.get(key));
}

export function useClickResult(model: AnyModel<WidgetModel>): (result: PickingResult) => void {
  return React.useCallback((result: PickingResult) => {
    model.set("click_result", result);
    model.save_changes();
  }, [model]);
}

type ModelBrush = {
  brush: BrushState | NoBrush;
  onBrush: (state: BrushState) => BrushResult;
  onBrushEnd: (state: BrushState, brushingResult: BrushingResult | undefined) => BrushResult;
  onBrushClear: () => void;
};

// The brush is always controlled, so that a brush set from the kernel is drawn.
// Only completed brushes are synced, since mid-drag states would flood the kernel.
export function useModelBrush(model: AnyModel<WidgetModel>): ModelBrush {
  const [brush, setBrush] = React.useState<BrushState | NoBrush>(() => model.get("brush") ?? NO_BRUSH);

  React.useEffect(() => {
    function onModelBrushChange() {
      setBrush(model.get("brush") ?? NO_BRUSH);
    }
    model.on("change:brush", onModelBrushChange);
    return () => model.off("change:brush", onModelBrushChange);
  }, [model]);

  const onBrush = React.useCallback((state: BrushState) => {
    setBrush(state);
    return {};
  }, []);

  const onBrushEnd = React.useCallback((state: BrushState, brushingResult: BrushingResult | undefined) => {
    setBrush(state);
    model.set("brush", state);
    model.set("brushing_result", brushingResult ?? null);
    model.save_changes();
    return {};
  }, [model]);

  const onBrushClear = React.useCallback(() => {
    setBrush(NO_BRUSH);
    model.set("brush", null);
    model.set("brushing_result", null);
    model.save_changes();
  }, [model]);

  return { brush, onBrush, onBrushEnd, onBrushClear };
}

type ModelCamera = {
  cameraMatrix: CameraOrExtent;
  setCameraMatrix: (next: CameraMatrix) => void;
  // Bumped on every refit, since <Pluot/> otherwise reuses its earlier fit.
  extentKey: string;
};

function getLimits(model: AnyModel<WidgetModel>): CameraOrExtent {
  return { xLim: model.get("x_lim"), yLim: model.get("y_lim") };
}

// A null camera_view, or a change to x_lim/y_lim, asks <Pluot/> to fit the
// camera to the limits, using the data extent for any omitted axis. The fitted
// matrix is then synced back to camera_view.
export function useThrottledCameraMatrix(model: AnyModel<WidgetModel>): ModelCamera {
  const [cameraMatrix, setLocalCameraMatrix] = React.useState<CameraOrExtent>(() => {
    const cameraView = model.get("camera_view");
    return cameraView === null ? getLimits(model) : Float32Array.from(cameraView);
  });
  const [fitCount, setFitCount] = React.useState(0);
  const pendingRef = React.useRef<CameraMatrix | null>(null);
  const lastSentRef = React.useRef<number[] | null>(null);
  const timeoutRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  React.useEffect(() => {
    function refit() {
      pendingRef.current = null;
      setLocalCameraMatrix(getLimits(model));
      setFitCount(count => count + 1);
    }
    function onCameraViewChange() {
      const cameraView = model.get("camera_view");
      if (cameraView === null) {
        refit();
        return;
      }
      // Kernel updates are ignored mid-interaction, otherwise stale echoes of
      // earlier syncs would make the camera jump backwards.
      if (timeoutRef.current !== null || cameraView === lastSentRef.current) return;
      setLocalCameraMatrix(Float32Array.from(cameraView));
    }
    model.on("change:camera_view", onCameraViewChange);
    model.on("change:x_lim", refit);
    model.on("change:y_lim", refit);
    return () => {
      model.off("change:camera_view", onCameraViewChange);
      model.off("change:x_lim", refit);
      model.off("change:y_lim", refit);
      if (timeoutRef.current !== null) clearTimeout(timeoutRef.current);
    };
  }, [model]);

  const setCameraMatrix = React.useCallback((next: CameraMatrix) => {
    setLocalCameraMatrix(next);
    pendingRef.current = next;

    function syncPending() {
      const pending = pendingRef.current;
      if (pending === null) {
        timeoutRef.current = null;
        return;
      }
      pendingRef.current = null;
      timeoutRef.current = setTimeout(syncPending, model.get("camera_sync_interval"));
      lastSentRef.current = Array.from(pending);
      model.set("camera_view", lastSentRef.current);
      model.save_changes();
    }

    if (timeoutRef.current === null) syncPending();
  }, [model]);

  return { cameraMatrix, setCameraMatrix, extentKey: String(fitCount) };
}

export function useExtentResult(model: AnyModel<WidgetModel>): (result: ExtentResult) => void {
  return React.useCallback((result: ExtentResult) => {
    model.set("extent", result);
    model.save_changes();
  }, [model]);
}
