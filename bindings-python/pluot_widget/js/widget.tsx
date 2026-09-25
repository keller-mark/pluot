import React, { useMemo } from "react";
import { createRender, useModel, useModelState } from "@anywidget/react";
import type { AnyModel } from "@anywidget/types";
import * as uuid from "@lukeed/uuid";
import {
  Pluot,
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

type WidgetModel = {
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

function createZarrGetter(model: AnyModel<WidgetModel>) {
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
function createKernelStore(zarrGet: ReturnType<typeof createZarrGetter>, storeName: string): AsyncReadable {
  return {
    get: (key) => zarrGet([storeName, key]),
    getRange: (key, rangeQuery) => zarrGet([storeName, key, rangeQuery]),
  };
}

function PluotWidget() {
  const model = useModel<WidgetModel>();
  const [width] = useModelState<number>("width");
  const [height] = useModelState<number>("height");
  const [cameraView, setCameraView] = useModelState<number[]>("camera_view");
  const [marginTop] = useModelState<number>("margin_top");
  const [marginRight] = useModelState<number>("margin_right");
  const [marginBottom] = useModelState<number>("margin_bottom");
  const [marginLeft] = useModelState<number>("margin_left");
  const [aspectRatioMode] = useModelState<AspectRatioMode>("aspect_ratio_mode");
  const [aspectRatioAlignmentMode] = useModelState<AspectRatioAlignmentMode>("aspect_ratio_alignment_mode");
  const [viewMode] = useModelState<ViewMode>("view_mode");
  const [plotId] = useModelState<string>("plot_id");
  const [plotType] = useModelState<PlotType>("plot_type");
  const [storesMetadata] = useModelState<StoresInput>("stores_metadata");
  const [plotParams] = useModelState<PlotParams>("plot_params");
  const [format] = useModelState<GraphicsFormat>("format");

  const zarrGet = useMemo(() => createZarrGetter(model), [model]);

  // Registered during render rather than in an effect, so that the kernel-backed
  // stores exist before <Pluot/> issues its first read.
  useMemo(() => {
    for (const storeName of Object.keys(storesMetadata ?? {})) {
      setStoreByName(storeName, createKernelStore(zarrGet, storeName));
    }
  }, [zarrGet, storesMetadata]);

  const cameraMatrix = useMemo(() => Float32Array.from(cameraView), [cameraView]);

  function setCameraMatrix(next: CameraMatrix) {
    setCameraView(Array.from(next));
  }

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
    />
  );
}

export default { render: createRender(PluotWidget) };
