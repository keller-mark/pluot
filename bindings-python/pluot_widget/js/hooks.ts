import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { AnyModel } from "@anywidget/types";
import * as uuid from "@lukeed/uuid";
import {
  setStoreByName,
  type AspectRatioAlignmentMode,
  type AspectRatioMode,
  type CameraMatrix,
  type GraphicsFormat,
  type PlotParams,
  type PlotType,
  type StoresInput,
  type ViewMode,
} from "@pluot/react";

type AsyncReadable = Parameters<typeof setStoreByName>[1];
type RangeQuery = Parameters<NonNullable<AsyncReadable["getRange"]>>[1];

type ZarrGetParams = [storeName: string, key: string] | [storeName: string, key: string, rangeQuery: RangeQuery];
type CommandResponse = { success: boolean };

export type WidgetModel = {
  width: number;
  height: number;
  camera_view: number[];
  margin_top: number;
  margin_right: number;
  margin_bottom: number;
  margin_left: number;
  aspect_ratio_mode: AspectRatioMode;
  aspect_ratio_alignment_mode: AspectRatioAlignmentMode;
  view_mode: ViewMode;
  plot_id: string;
  plot_type: PlotType;
  stores_metadata: StoresInput;
  plot_params: PlotParams;
  format: GraphicsFormat;
  batch_zarr_gets: boolean;
  // Milliseconds.
  min_timeout: number;
  // Milliseconds.
  camera_sync_interval: number;
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
  const subscribe = useCallback((onChange: () => void) => {
    model.on(`change:${key}`, onChange);
    return () => model.off(`change:${key}`, onChange);
  }, [model, key]);
  return useSyncExternalStore(subscribe, () => model.get(key));
}

export function useThrottledCameraMatrix(model: AnyModel<WidgetModel>): [CameraMatrix, (next: CameraMatrix) => void] {
  const [cameraMatrix, setLocalCameraMatrix] = useState<CameraMatrix>(
    () => Float32Array.from(model.get("camera_view")),
  );
  const pendingRef = useRef<CameraMatrix | null>(null);
  const lastSentRef = useRef<number[] | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    function onCameraViewChange() {
      const cameraView = model.get("camera_view");
      // Kernel updates are ignored mid-interaction, otherwise stale echoes of
      // earlier syncs would make the camera jump backwards.
      if (timeoutRef.current !== null || cameraView === lastSentRef.current) return;
      setLocalCameraMatrix(Float32Array.from(cameraView));
    }
    model.on("change:camera_view", onCameraViewChange);
    return () => {
      model.off("change:camera_view", onCameraViewChange);
      if (timeoutRef.current !== null) clearTimeout(timeoutRef.current);
    };
  }, [model]);

  const setCameraMatrix = useCallback((next: CameraMatrix) => {
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

  return [cameraMatrix, setCameraMatrix];
}
