import React, { useLayoutEffect, useEffect, useEffectEvent, useRef, useState, useMemo, useId, type CSSProperties } from "react";
import lzs from "lz-string";
import { throttle } from "lodash-es";
import { useQuery, useQueryClient, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  initialize, getIsWasmReady,
  render_wasm, pick_wasm, brush_wasm, extent_wasm,
  normalizeStores, getStore,
  checkWebGpuFeatureDetection,
  onMouseMove2d, onWheel2d,
  onMouseMove3d, onWheel3d,
  getCameraMatrixFromBounds,
  type CameraMatrix, type Bounds, type AspectRatioMode, type AspectRatioAlignmentMode,
} from '@pluot/core';
import { Tooltip } from "./Tooltip.js";
import { BrushOverlay } from "./BrushOverlay.js";
import { useBrush } from "./use-brush.js";
import { useMemoCustomComparison } from "./use-memo-custom.js";
import type {
  BrushingResult, BrushState, ExtentResult, GraphicsFormat, HoverInfo, PickingResult, PlotParams, PlotType,
  PluotProps, RawBrushingResult, RawPickingResult, RenderParams, TooltipContent,
} from "./types.js";

// Needed due to "SyntaxError: Named export 'decompressFromUint8Array' not found.
// The requested module 'lz-string' is a CommonJS module,
// which may not support all module.exports as named exports."
const { decompressFromUint8Array } = lzs;

const DEFAULT_VIEW = new Float32Array([
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1/200, 0,
  0, 0, 0, 1,
]);


const DEFAULT_3D_VIEW = new Float32Array([
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1, 0,
  0, 0, -10, 1,
]);

const identity = <T,>(param: T): T => param;
const noop = () => { };

// Mouse movement (in pixels) beyond which a mousedown-to-click is
// considered a drag rather than a click, so that picking is skipped.
const DRAG_THRESHOLD_PX = 3;

// How often rapid-fire prop changes (e.g. `cameraMatrix` while dragging) are
// coalesced into a single render_wasm call; see `throttledSetRenderParamsSnapshot`.
const RENDER_PARAMS_THROTTLE_MS = 16; // ~60fps

// Thrown by the render `useQuery`'s `queryFn` when a frame bails early (still
// waiting on underlying store fetches), so that react-query's own `retry`/
// `retryDelay` drive the "keep re-rendering with a growing backoff" loop,
// rather than a manually-managed poll.
class RenderBailedEarlyError extends Error {
  constructor() {
    super("Render bailed early: still waiting on data.");
    this.name = "RenderBailedEarlyError";
  }
}

// The subset of render/pick/brush params that determine what gets drawn,
// snapshotted (and throttled, see `throttledSetRenderParamsSnapshot`) so that
// rapid-fire prop changes (e.g. `cameraMatrix` during a drag) collapse into
// at most one render_wasm call per `RENDER_PARAMS_THROTTLE_MS`.
type RenderParamsSnapshot = {
  plotId: string;
  plotType: PlotType;
  plotParams: PlotParams;
  stores: RenderParams["stores"];
  format: GraphicsFormat;
  width: number;
  height: number;
  aspectRatioMode: AspectRatioMode;
  aspectRatioAlignmentMode: AspectRatioAlignmentMode;
  marginLeft: number;
  marginRight: number;
  marginTop: number;
  marginBottom: number;
  cameraMatrix: number[];
};

// The same params, plus the screen coordinates being picked: a pick result is
// a pure function of these, so a click/hover at the same coordinates with the
// same render params can reuse a cached result instead of re-invoking wasm.
type PickParamsSnapshot = RenderParamsSnapshot & {
  screenCoordX: number;
  screenCoordY: number;
};

// `pick_wasm` is typed `any` by wasm-bindgen, so `RawPickingResult` is what
// documents its wire format (see types.ts).
function normalizePickingResult(data: RawPickingResult): PickingResult {
  return {
    ...data,
    layer_results: data.layer_results.map(({ layer_id, info }) => ({
      layer_id,
      // This is needed because serde-wasm-bindgen
      // converts Rust HashMap to JS Map.
      info: Object.fromEntries(info),
    })),
  };
}

// `brush_wasm` is typed `any` by wasm-bindgen, so `RawBrushingResult` is what
// documents its wire format (see types.ts).
function normalizeBrushingResult(data: RawBrushingResult): BrushingResult {
  return {
    ...data,
    layer_results: data.layer_results.map(({ layer_id, info, element_info }) => ({
      layer_id,
      // This is needed because serde-wasm-bindgen
      // converts Rust HashMap to JS Map.
      info: Object.fromEntries(info),
      element_info: Object.fromEntries(element_info),
    })),
  };
}

// The union (bounding box) of every layer's reported extent, in each axis
// independently. `null` for an axis when no layer in the plot reports an
// extent for it (e.g. none of them implement `ExtentableLayer` yet).
function unionExtent(result: ExtentResult | undefined): { x: [number, number] | null, y: [number, number] | null } {
  if (!result || result.layer_results.length === 0) {
    return { x: null, y: null };
  }

  let xMin = Infinity, xMax = -Infinity, yMin = Infinity, yMax = -Infinity;
  for (const { x, y } of result.layer_results) {
    xMin = Math.min(xMin, x[0]);
    xMax = Math.max(xMax, x[1]);
    yMin = Math.min(yMin, y[0]);
    yMax = Math.max(yMax, y[1]);
  }
  return { x: [xMin, xMax], y: [yMin, yMax] };
}

function isArray(arr: any) {
  // We use this when checking whether cameraMatrix.camera is an array,
  // since the camera matrix may be a typed array,
  // and Array.isArray returns false for typed arrays,
  return Array.isArray(arr) || arr instanceof Float32Array;
}


function PluotInner(props: PluotProps) {
  const {
    schemaVersion = null,
    width: widthProp,
    height: heightProp,
    plotId,
    plotType,
    store: storeProp,
    storeName: storeNameProp,
    stores: storesProp,
    registerStores = true,
    plotParams,
    viewMode = "2d",
    marginBottom = 100.0,
    marginLeft = 100.0,
    marginTop = 100.0,
    marginRight =  100.0,
    aspectRatioMode = "Contain", // "Ignore", "Contain", "Cover"
    aspectRatioAlignmentMode = "Start", // "Center", "Start", "End"
    format = "Raster", // "Raster", "Vector"
    minTimeout = 32,
    maxTimeout = 5000,
    maxBailedEarlyRetries = 300,
    allowSimultaneousRenders = true,
    debugMargins = false,
    backgroundColor = undefined,
    cameraMatrix: cameraMatrixPropRaw = null,
    setCameraMatrix: setCameraMatrixProp = null,
    enableExtentQuery = true,
    enableClick = false,
    enableTooltip = false,
    onClick: onClickProp = null,
    onHover: onHoverProp = null,
    brushUnitsModeX = "Data",
    brushUnitsModeY = "Data",
    brushMarginTop,
    brushMarginRight,
    brushMarginBottom,
    brushMarginLeft,
    enableBrushCreate = false,
    enableBrushEdit = false,
    enableBrushClear = false,
    brushDelay = 1500,
    maybeBrushDelay = 250,
    persistBrush = false,
    brushMode = "Rect",
    brushColor = "#3b6ea5",
    // An omitted `brush` means uncontrolled; a controlled parent signals the
    // empty state with `NO_BRUSH`, never `undefined`.
    brush = null,
    onBrush,
    onBrushEnd,
    onBrushClear,

    // Temporary workaround. See comments in LruStore.clearCache.
    shouldClearCache = true,
  } = props;

  const width = Math.floor(widthProp);
  const height = Math.floor(heightProp);

  const isVector = format === "Vector";

  const onClick: (result: PickingResult) => void = typeof onClickProp === 'function' ? onClickProp : noop;
  const onHover: (result: PickingResult) => TooltipContent = typeof onHoverProp === 'function' ? onHoverProp : identity;

  const [isWasmReady, setIsWasmReady] = useState(false);
  const [supportsWebGpu, supportsWebGpuMessage] = useMemo(checkWebGpuFeatureDetection, []);

  // Ensure that the cameraMatrix prop equality is not based on the object reference,
  // as it may change on every render despite the internal camera property reference
  // being stable.
  const cameraMatrixProp = useMemoCustomComparison(() => {
    return cameraMatrixPropRaw;
  }, { cameraMatrixPropRaw }, (prevDeps, nextDeps) => {
    const prevCamera = prevDeps.cameraMatrixPropRaw;
    const nextCamera = nextDeps.cameraMatrixPropRaw;
    if (prevCamera && 'camera' in prevCamera && nextCamera && 'camera' in nextCamera) {
      return prevCamera.camera === nextCamera.camera;
    }
    // TODO: custom equality checks for xLim/yLim properties?
    return prevCamera === nextCamera;
  });

  // Initialize the WASM module.
  useLayoutEffect(() => {
    initialize().then(() => setIsWasmReady(getIsWasmReady()));
  }, []);

  // We may want to update the timeout duration without triggering a re-render.
  const currentTimeout = useRef(minTimeout);

  // If this is false, then we need to wait for the extent_wasm result.
  const hasCompleteCameraParams = (
    cameraMatrixProp && (
      // When false, the user has explicitly told us to use the identity camera matrix rather than extent_wasm.
      !enableExtentQuery
      // Camera matrix is provided OR both xLim and yLim are provided.
      || ('camera' in cameraMatrixProp && isArray(cameraMatrixProp.camera))
      || ('xLim' in cameraMatrixProp && isArray(cameraMatrixProp.xLim) && 'yLim' in cameraMatrixProp && isArray(cameraMatrixProp.yLim))
    )
  );

  // Whether we actually need the fetched full extent to compute the camera
  // bounds below: not when the camera is user-controlled, and not when both
  // `xLim`/`yLim` are already given explicitly.
  const extentQueryEnabled = enableExtentQuery && isWasmReady && !hasCompleteCameraParams;

  // Runs the extent query against the wasm module. Excludes `plotType`/
  // `plotParams`/`stores` from the key: for now, `plotId` alone is used to
  // invalidate the extent (see the `applyLim`-triggering effect below).
  const extentQuery = useQuery({
    queryKey: ['pluot-extent', plotId, width, height, aspectRatioMode, aspectRatioAlignmentMode,
      marginTop, marginRight, marginBottom, marginLeft],
    queryFn: async (): Promise<ExtentResult> => {
      const renderParams: RenderParams = {
        schema_version: schemaVersion,
        width,
        height,
        format: format,
        margin_bottom: marginBottom,
        margin_left: marginLeft,
        margin_top: marginTop,
        margin_right: marginRight,
        device_pixel_ratio: window.devicePixelRatio,
        aspect_ratio_mode: aspectRatioMode,
        aspect_ratio_alignment_mode: aspectRatioAlignmentMode,
        view_mode: viewMode,
        pickable: false,
        camera_view: DEFAULT_VIEW,
        plot_id: plotId,
        plot_type: plotType,
        stores,
        plot_params: plotParams,
        timeout: null, // Note: no timeout
        wait_for_store_gets: false,
        cache_enabled: true,
        svg_compression_enabled: true,
        svg_include_document: false,
      };

      // Unlike `pick_wasm`/`brush_wasm`, `ExtentResult` has no `HashMap` fields,
      // so `serde_wasm_bindgen` produces plain objects/arrays directly and no
      // normalization step is needed.
      return await extent_wasm(renderParams) as ExtentResult;
    },
    enabled: extentQueryEnabled,
  });

  const initialCameraMatrix = useMemo(() => {
    console.log("useMemo: initialCameraMatrix", cameraMatrixProp, enableExtentQuery);
    if (cameraMatrixProp && 'camera' in cameraMatrixProp && isArray(cameraMatrixProp.camera)) {
      // Full camera matrix was provided up-front.
      return Float32Array.from(cameraMatrixProp.camera);
    }

    if (!enableExtentQuery && (!cameraMatrixProp || 'camera' in cameraMatrixProp && !cameraMatrixProp.camera)) {
      // Camera matrix was not provided, but the user does not want to use extent_wasm.
      return Float32Array.from(
        viewMode === "2d" ? DEFAULT_VIEW : DEFAULT_3D_VIEW
      );
    }

    const hasXlim = cameraMatrixProp && 'xLim' in cameraMatrixProp && isArray(cameraMatrixProp.xLim);
    const hasYlim = cameraMatrixProp && 'yLim' in cameraMatrixProp && isArray(cameraMatrixProp.yLim);

    // If we have BOTH xlim and ylim, then we do not need the extent_was result at all.
    const needsExtentResult = !hasXlim || !hasYlim;
    const hasExtentResult = extentQuery.data && extentQuery.isSuccess;

    const bounds: Bounds = {};

    if (hasXlim) {
      bounds.xMin = cameraMatrixProp.xLim?.[0];
      bounds.xMax = cameraMatrixProp.xLim?.[1];
    }

    if (hasYlim) {
      bounds.yMin = cameraMatrixProp.yLim?.[0];
      bounds.yMax = cameraMatrixProp.yLim?.[1];
    }

    if (!needsExtentResult) {
      const computedCameraMatrix = getCameraMatrixFromBounds(bounds, DEFAULT_VIEW, {
        width, height, aspectRatioMode, aspectRatioAlignmentMode,
        margins: { marginTop, marginRight, marginBottom, marginLeft },
      });
      return Float32Array.from(computedCameraMatrix);
    }


    // Needs extent result.
    if (!hasExtentResult) {
      return undefined;
    }

    // TODO: generalize to 3D
    // TODO: validate the contents of xLimProp/yLimProp (arrays with two numeric elements).

    const extentResult = extentQuery.data;
    if (!extentResult || extentResult.layer_results.length === 0) {
      return Float32Array.from(DEFAULT_VIEW);
    }

    const fullExtent = unionExtent(extentResult);
    if (!hasXlim) {
      if(fullExtent.x) {
        bounds.xMin = fullExtent.x[0];
        bounds.xMax = fullExtent.x[1];
      } else {
        console.log("Warning: fullExtent.x was not computed.");
      }
    }
    if (!hasYlim) {
      if(fullExtent.y) {
        bounds.yMin = fullExtent.y[0];
        bounds.yMax = fullExtent.y[1];
      } else {
        console.log("Warning: fullExtent.y was not computed.");
      }
    }
    const computedCameraMatrix = getCameraMatrixFromBounds(bounds, DEFAULT_VIEW, {
      width, height, aspectRatioMode, aspectRatioAlignmentMode,
      margins: { marginTop, marginRight, marginBottom, marginLeft },
    });
    return Float32Array.from(computedCameraMatrix);
  }, [extentQueryEnabled, hasCompleteCameraParams, extentQuery.data, extentQuery.isSuccess]);

  const hasFullCameraMatrixProp = cameraMatrixProp && 'camera' in cameraMatrixProp && isArray(cameraMatrixProp.camera);

  // If cameraMatrix is not provided, then we manage the camera matrix internally.
  const [uncontrolledCameraMatrix, setUncontrolledCameraMatrix] = useState<CameraMatrix | undefined>(initialCameraMatrix);

  useEffect(() => {
    console.log("useEffect: setUncontrolledCameraMatrix")
    setUncontrolledCameraMatrix(prev => prev === undefined ? initialCameraMatrix : prev);
  }, [initialCameraMatrix]);

  // Decide which camera matrix and setter to use.
  // If the user provides the cameraMatrix prop but NOT the setCameraMatrix setter,
  // then interpret the prop as the "initial" camera settings, but still treat as uncontrolled.
  const isControlledCamera = typeof setCameraMatrixProp === "function";

  // Alternatively, if the user provides the setCameraMatrix setter, but NOT
  // the cameraMatrix, interpret this as they want to use the default camera
  // value initially, but they still want a controlled camera matrix.
  const cameraMatrix = isControlledCamera ? (
    hasFullCameraMatrixProp
    ? cameraMatrixProp.camera
    : initialCameraMatrix
  ) : uncontrolledCameraMatrix;

  const setCameraMatrix: (nextCameraMatrix: CameraMatrix) => void = isControlledCamera
    ? setCameraMatrixProp
    : setUncontrolledCameraMatrix;

  // If this is false, then we cannot render anything, as we are still awaiting the extent_wasm call.
  const hasCameraMatrix = cameraMatrix !== undefined;

  // Build the top-level `stores` map that RenderParams expects: a mapping from
  // store name to its derived `ZarrStoreInfo` metadata.
  const stores = useMemo(() => normalizeStores({
    stores: storesProp,
    store: storeProp,
    storeName: storeNameProp,
    plotId,
    register: registerStores,
  }), [storeNameProp, storeProp, storesProp, plotId, registerStores]);

  const svgRef = useRef<SVGSVGElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const cameraElementRef = useRef<HTMLDivElement | null>(null);
  // The outer (width x height) element, which is the coordinate space that both
  // the brush overlay and the hover tooltip are positioned within.
  const containerRef = useRef<HTMLDivElement | null>(null);

  const tempButtonRef = useRef<HTMLButtonElement | null>(null);



  // Used to distinguish a plain click from a click that ends a drag
  // (e.g. panning), so that dragging does not trigger picking.
  const dragStartRef = useRef<{ x: number, y: number } | null>(null);
  const didDragRef = useRef(false);


  const [didFirstRender, setDidFirstRender] = useState(false);
  const [bailedEarly, setBailedEarly] = useState(true);

  // hoverInfo.mouseX/mouseY are in the coordinate space of the outer
  // (width x height) container, used to position the hover tooltip.
  const [hoverInfo, setHoverInfo] = useState<HoverInfo | null>(null);

  const progressBarId = useId();

  // Runs the brush query against the wasm module for a given brush state,
  // analogous to `pick` below (defined here, ahead of `pick`, since `useBrush`
  // needs it immediately).
  const runBrush = useEffectEvent(async (state: BrushState): Promise<undefined> => {
    // TODO: remove this
  });

  const {
    brushState,
    overlayRef: brushOverlayRef,
    geometry: brushGeometry,
    pressProgress,
    isBrushHovered,
    isBrushingRef,
    shouldSuppressClickRef,
    onVertexMouseDown,
    onEdgeMouseDown,
    onClearClick,
  } = useBrush({
    containerRef,
    width, height,
    marginTop, marginRight, marginBottom, marginLeft,
    aspectRatioMode, aspectRatioAlignmentMode,
    // Note: the false branch should not be hit here either.
    cameraMatrix: cameraMatrix ? cameraMatrix : DEFAULT_VIEW,
    brushUnitsModeX, brushUnitsModeY,
    brushMarginTop, brushMarginRight, brushMarginBottom, brushMarginLeft,
    enableBrushCreate, enableBrushEdit, enableBrushClear,
    brushDelay, maybeBrushDelay, persistBrush, brushMode,
    brush, onBrush, onBrushEnd, onBrushClear, runBrush,
  });


  const wheelHandler = useEffectEvent((event: WheelEvent) => {
    if (!cameraMatrix) {
      // Still awaiting extent_wasm.
      return;
    }
    const onWheel = viewMode === "3d" ? onWheel3d : onWheel2d;
    const nextCameraMatrix = onWheel({
        width,
        height,
        aspectRatioMode,
        aspectRatioAlignmentMode,
        margins: {
          marginTop,
          marginBottom,
          marginLeft,
          marginRight,
        },
      }, cameraMatrix, event);
    setCameraMatrix(nextCameraMatrix);
  });

  const mouseMoveHandler = useEffectEvent((event: MouseEvent) => {
    if (!cameraMatrix) {
      // Still awaiting extent_wasm.
      return;
    }

    // A drag that is drawing or editing a brush must not also pan/rotate the camera.
    if (isBrushingRef.current) {
      return;
    }
    const onMouseMove = viewMode === "3d" ? onMouseMove3d : onMouseMove2d;
    const nextCameraMatrix = onMouseMove({
        width,
        height,
        aspectRatioMode,
        aspectRatioAlignmentMode,
        margins: {
          marginTop,
          marginBottom,
          marginLeft,
          marginRight,
        },
      }, cameraMatrix, event);
    setCameraMatrix(nextCameraMatrix);
  });

  // Builds the params snapshot for a pick at the given screen coordinates,
  // capturing the current render-affecting props plus the coordinates.
  const buildPickParamsSnapshot = (screenCoordX: number, screenCoordY: number): PickParamsSnapshot => ({
    plotId, plotType, plotParams, stores, format, width, height,
    aspectRatioMode, aspectRatioAlignmentMode,
    marginLeft, marginRight, marginTop, marginBottom,
    // Note: The negative branch should never be reached here as long as we don't trigger picking when camera matrix is not yet defined.
    cameraMatrix: cameraMatrix ? Array.from(cameraMatrix) : Array.from(DEFAULT_VIEW),
    screenCoordX, screenCoordY,
  });

  // Runs one pick_wasm call for a given params snapshot. Reads `schemaVersion`,
  // `viewMode`, and `currentTimeout` directly from the enclosing scope rather
  // than the snapshot (matching the render query's `renderFrame`), since a
  // fresh closure is handed to `useQuery` on every render.
  const runPick = async (params: PickParamsSnapshot): Promise<PickingResult> => {
    const renderParams: RenderParams = {
      schema_version: schemaVersion,
      width: params.width,
      height: params.height,
      format: params.format,
      margin_bottom: params.marginBottom,
      margin_left: params.marginLeft,
      margin_top: params.marginTop,
      margin_right: params.marginRight,
      device_pixel_ratio: window.devicePixelRatio,
      aspect_ratio_mode: params.aspectRatioMode,
      aspect_ratio_alignment_mode: params.aspectRatioAlignmentMode,
      view_mode: viewMode,
      pickable: false,
      camera_view: Float32Array.from(params.cameraMatrix),
      plot_id: params.plotId,
      plot_type: params.plotType,
      stores: params.stores,
      plot_params: params.plotParams,
      // Reduce the timeout value to improve responsiveness during data loading (bailed-early renders)?
      timeout: currentTimeout.current, // in ms // Note: will not have any effect when wait_for_store_gets is false.
      wait_for_store_gets: false, // TODO: lift this value up to pass/use it in the window.zarr_ functions as well?
      cache_enabled: true,
      svg_compression_enabled: true,
      svg_include_document: false,
    };

    const layerHeight = params.height - params.marginTop - params.marginBottom;

    // TODO: wrap pick_wasm in a try/catch

    return normalizePickingResult(await pick_wasm(
      renderParams,
      // The coordinates are relative to the "layer" (the camera region), not the full width/height.
      // We also need to flip the Y coordinate so that positive is up.
      params.screenCoordX + params.marginLeft,
      params.marginBottom + (layerHeight - params.screenCoordY)
    ));
  };

  // The most recently requested click/hover pick. Kept (rather than reset to
  // null once consumed below) so that a repeated pick at the same coordinates
  // with the same render params can be served from the query cache.
  const [clickPickParams, setClickPickParams] = useState<PickParamsSnapshot | null>(null);
  const [hoverPickParams, setHoverPickParams] = useState<PickParamsSnapshot | null>(null);

  const clickPickQuery = useQuery({
    queryKey: ['pluot-pick', clickPickParams],
    queryFn: () => runPick(clickPickParams!),
    enabled: clickPickParams !== null && cameraMatrix !== undefined,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    retry: false,
    gcTime: 0,
  });

  const hoverPickQuery = useQuery({
    queryKey: ['pluot-pick', hoverPickParams],
    queryFn: () => runPick(hoverPickParams!),
    enabled: hoverPickParams !== null && cameraMatrix !== undefined,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    retry: false,
    gcTime: 0,
  });

  // Fires the click callback once the query for the latest click resolves.
  // Since `clickPickQuery` tracks whichever key `clickPickParams` currently
  // holds, a click that becomes stale (superseded by a newer one before it
  // resolves) never reaches here: its result would land on a cache entry this
  // query has already stopped observing.
  useEffect(() => {
    console.log("useEffect: clickPickParams")
    if (clickPickParams !== null && clickPickQuery.data !== undefined) {
      onClick(clickPickQuery.data);
    }
  }, [clickPickParams, clickPickQuery.data]);

  // The click-picking callback.
  const pickFrame = useEffectEvent((screenCoordX: number, screenCoordY: number) => {
    console.log("useEffectEvent: setClickPickParams");
    setClickPickParams(buildPickParamsSnapshot(screenCoordX, screenCoordY));
  });

  // Fires the hover callback once the query for the latest hover resolves.
  useEffect(() => {
    if (!cameraMatrix) {
      return;
    }
    if (hoverPickParams !== null && hoverPickQuery.data !== undefined) {
      setHoverInfo({
        content: onHover(hoverPickQuery.data),
        // Convert from cameraEl-relative coordinates to outer-container-relative
        // coordinates, since the tooltip is positioned within the outer container.
        mouseX: hoverPickParams.screenCoordX + hoverPickParams.marginLeft,
        mouseY: hoverPickParams.screenCoordY + hoverPickParams.marginTop,
      });
    }
  }, [hoverPickParams, hoverPickQuery.data]);

  // The hover-picking callback.
  const hoverFrame = useEffectEvent((screenCoordX: number, screenCoordY: number) => {
    if (!cameraMatrix) {
      return;
    }
    setHoverPickParams(buildPickParamsSnapshot(screenCoordX, screenCoordY));
  });

  const throttledHoverFrame = useMemo(
    () => throttle(
      hoverFrame,
      50,
      { leading: true, trailing: true },
    ), []);

  useEffect(() => {
    return () => throttledHoverFrame.cancel();
  }, [throttledHoverFrame]);

  // Set up the camera and picking handlers.
  useEffect(() => {
    const cameraEl = cameraElementRef.current;

    if (!cameraEl) {
      return () => {};
    }

    // Create a 2D camera for handling zoom and pan.
    cameraEl.addEventListener("mousemove", mouseMoveHandler);
    cameraEl.addEventListener("wheel", wheelHandler);

    // Track mousedown -> mousemove distance so that a drag (e.g. panning)
    // that ends on the camera element does not also trigger a click/pick.
    const mouseDownHandler = (event: MouseEvent) => {
      dragStartRef.current = { x: event.clientX, y: event.clientY };
      didDragRef.current = false;
      // A brush drag that ended outside the camera element never produced the
      // click that would have consumed this flag, so clear it as the next
      // interaction begins rather than letting it suppress that one too.
      shouldSuppressClickRef.current = false;
    };
    const dragDetectHandler = (event: MouseEvent) => {
      if (!dragStartRef.current) {
        return;
      }
      const dx = event.clientX - dragStartRef.current.x;
      const dy = event.clientY - dragStartRef.current.y;
      if (Math.hypot(dx, dy) > DRAG_THRESHOLD_PX) {
        didDragRef.current = true;
      }
    };
    cameraEl.addEventListener("mousedown", mouseDownHandler);
    cameraEl.addEventListener("mousemove", dragDetectHandler);

    // Set up an onClick handler for picking.
    const clickHandler = (event: MouseEvent) => {
      const wasDrag = didDragRef.current;
      // A brush drag (or a click on the clear button) ends with a click on the
      // camera element, which should not also run a picking query.
      const wasBrush = shouldSuppressClickRef.current;
      dragStartRef.current = null;
      didDragRef.current = false;
      shouldSuppressClickRef.current = false;
      if (enableClick && !wasDrag && !wasBrush) {
        pickFrame(event.offsetX, event.offsetY);
      }
    };
    cameraEl.addEventListener("click", clickHandler);

    // Set up hover handlers for picking, only when the onHover prop is provided.
    const hoverMoveHandler = (event: MouseEvent) => {
      if (enableTooltip && !isBrushingRef.current) {
        throttledHoverFrame(event.offsetX, event.offsetY);
      }
    };
    const hoverLeaveHandler = () => {
      throttledHoverFrame.cancel();
      setHoverInfo(null);
    };
    if (enableTooltip) {
      cameraEl.addEventListener("mousemove", hoverMoveHandler);
      cameraEl.addEventListener("mouseleave", hoverLeaveHandler);
    }

    return () => {
      cameraEl.removeEventListener("mousemove", mouseMoveHandler);
      cameraEl.removeEventListener("wheel", wheelHandler);
      cameraEl.removeEventListener("mousedown", mouseDownHandler);
      cameraEl.removeEventListener("mousemove", dragDetectHandler);
      cameraEl.removeEventListener("click", clickHandler);
      cameraEl.removeEventListener("mousemove", hoverMoveHandler);
      cameraEl.removeEventListener("mouseleave", hoverLeaveHandler);
    };
  }, [viewMode, enableClick, enableTooltip, throttledHoverFrame]);


  // The params that determine what gets drawn, snapshotted so that a burst of
  // prop changes (e.g. `cameraMatrix` while dragging) collapses into at most
  // one render_wasm call per `RENDER_PARAMS_THROTTLE_MS` (see the throttled
  // setter below) and drives the render `useQuery`'s key.
  const [renderParamsSnapshot, setRenderParamsSnapshot] = useState<RenderParamsSnapshot|undefined>();

  // Runs one render_wasm call for a given params snapshot, throwing
  // `RenderBailedEarlyError` when the frame bails early so that `renderQuery`'s
  // `retry`/`retryDelay` (below) take over re-rendering. Reads `schemaVersion`
  // and `viewMode` directly from props rather than the snapshot (matching prior
  // behavior) since a fresh closure is handed to `useQuery` on every render, so
  // it always sees their latest values regardless of the query's cache key.
  const renderFrame = async (params: RenderParamsSnapshot | undefined): Promise<null> => {
    console.log('wasm.render');

    if (!params) {
      return null;
    }

    const renderParams: RenderParams = {
      schema_version: schemaVersion,
      width: params.width,
      height: params.height,
      format: params.format,
      margin_bottom: params.marginBottom,
      margin_left: params.marginLeft,
      margin_top: params.marginTop,
      margin_right: params.marginRight,
      device_pixel_ratio: window.devicePixelRatio,
      aspect_ratio_mode: params.aspectRatioMode,
      aspect_ratio_alignment_mode: params.aspectRatioAlignmentMode,
      view_mode: viewMode,
      pickable: false,
      camera_view: Float32Array.from(params.cameraMatrix),
      plot_id: params.plotId,
      plot_type: params.plotType,
      stores: params.stores,
      plot_params: params.plotParams,
      // Reduce the timeout value to improve responsiveness during data loading (bailed-early renders)?
      timeout: currentTimeout.current, // in ms // Note: will not have any effect when wait_for_store_gets is false.
      wait_for_store_gets: false, // TODO: lift this value up to pass/use it in the window.zarr_ functions as well?
      cache_enabled: true,
      svg_compression_enabled: true,
      svg_include_document: false,
    };

    // Wrap render_wasm in try/catch, to handle Rust panics; rethrow so the
    // query is marked errored rather than silently treated as a success.
    let arr: Uint8Array;
    try {
      arr = await render_wasm(renderParams);
    } catch (error) {
      console.error("Error during wasm.render_wasm:", error);
      throw error;
    }

    const frameBailedEarly = arr.at(-1) === 1;
    const graphicsArr = arr.subarray(0, -1);

    if (isVector) {
      // Format: Vector (render to SVG)
      const gContents = decompressFromUint8Array(graphicsArr);
      if (svgRef.current) {
        svgRef.current.innerHTML = gContents;
      }
    } else {
      // Format: Raster (render to canvas)
      const canvas = canvasRef.current;
      if (canvas) {
        const ctx = canvas.getContext("2d");
        if (ctx) {
          // TODO: is there a more efficient way to do this?
          // E.g., write to a webgl texture? or is this fast enough already?
          const imageData = new ImageData(
            new Uint8ClampedArray(graphicsArr),
            params.width,
            params.height,
          );
          ctx.putImageData(imageData, 0, 0);
        }
      }
    }

    setDidFirstRender(true);

    if (frameBailedEarly) {
      setBailedEarly(true); // Update this to show the loading indicator.
      throw new RenderBailedEarlyError();
    }

    // Successful render.
    currentTimeout.current = minTimeout;
    setBailedEarly(false); // Update this to hide the loading indicator.

    // Clear the LRU cache for the store (via its store_name) corresponding to the rendered plot.
    Object.keys(params.stores ?? {}).forEach(storeName => {
      const storeUsed = getStore(storeName);
      if (storeUsed && typeof storeUsed.clearCache === 'function' && shouldClearCache) {
        storeUsed.clearCache();
      }
    });

    return null;
  };

  const queryClient = useQueryClient();

  const renderQuery = useQuery({
    queryKey: ['pluot-render', renderParamsSnapshot],
    queryFn: (ctx) => {
      return renderFrame(renderParamsSnapshot);
    },
    enabled: isWasmReady && cameraMatrix !== undefined && renderParamsSnapshot !== undefined,
    // Keep retrying while the frame is bailing early, up to `maxBailedEarlyRetries`;
    // any other thrown error (e.g. a Rust panic) is left alone.
    retry: (failureCount, error) => error instanceof RenderBailedEarlyError && failureCount < maxBailedEarlyRetries,
    // Exponential backoff, doubling on every bailed-early retry: also used as
    // the `timeout` given to the *next* render_wasm/pick_wasm/brush_wasm call
    // (via the shared `currentTimeout` ref), so wasm is given roughly as long
    // to wait for the underlying store fetches as react-query waits to retry.
    retryDelay: (failureCount) => {
      const nextTimeout = Math.min(minTimeout * 2 ** failureCount, maxTimeout);
      currentTimeout.current = nextTimeout;
      return nextTimeout;
    },
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    gcTime: 0,
  });

  const throttledSetRenderParamsSnapshot = useMemo(
    () => throttle(
      // Reset the backoff timeout synchronously with every params change (rather
      // than in a separate effect keyed on `renderParamsSnapshot`), so the next
      // sequence of bailed-early renders starts from the minimum again before
      // react-query can act on the new snapshot, with no ordering race between
      // the two.
      (nextSnapshot: RenderParamsSnapshot) => {
        currentTimeout.current = minTimeout;
        // Evict prior render cache entries explicitly (on top of `gcTime: 0`,
        // which only collects them once no longer observed) so a params
        // change never risks serving a stale cached render.
        //queryClient.invalidateQueries({ queryKey: ['pluot-render'] });
        setRenderParamsSnapshot(nextSnapshot);
      },
      RENDER_PARAMS_THROTTLE_MS,
      // When both leading and trailing are true (the default):
      // - First call -> executes immediately (leading edge)
      // - Calls during the wait window -> ignored, but the most recent one is remembered.
      // - After the wait period expires -> the last remembered call is executed (trailing edge).
      { leading: true, trailing: true }
    ), []);

  useEffect(() => {
    return () => throttledSetRenderParamsSnapshot.cancel();
  }, [throttledSetRenderParamsSnapshot]);

  useEffect(() => {
    if (!isWasmReady) {
      return;
    }

    if (!cameraMatrix) {
      return;
    }

    // We want to allow for simultaneous renders, as this makes user interactions feel
    // much smoother. However, we allow for users to opt-out, and we also
    // need to prevent simultaneous renders prior to the first render, as the first
    // render initializes cached values and stuff.
    if (renderQuery.isFetching && (!didFirstRender || !allowSimultaneousRenders)) {
      // Prevent multiple render calls prior to the first successful render.
      return;
    }

    throttledSetRenderParamsSnapshot({
      plotId, plotType, plotParams, stores, format, width, height,
      aspectRatioMode, aspectRatioAlignmentMode,
      marginLeft, marginRight, marginTop, marginBottom,
      // Note: again, false branch should not be hit, as noted above.
      cameraMatrix: cameraMatrix ? Array.from(cameraMatrix) : Array.from(DEFAULT_VIEW),
    });
  }, [isWasmReady, renderQuery.isFetching, didFirstRender, bailedEarly, allowSimultaneousRenders,
    cameraMatrix, plotId, plotType, plotParams, stores, format,
    width, height, aspectRatioMode, aspectRatioAlignmentMode, marginLeft, marginRight, marginTop, marginBottom]);

  // Position the hover tooltip so that it grows diagonally away from whichever
  // quadrant of the plot the mouse currently occupies, to avoid clipping.
  const hoverStyle = useMemo<CSSProperties | null>(() => {
    if (!hoverInfo) {
      return null;
    }
    const { mouseX, mouseY } = hoverInfo;
    const isLeft = mouseX < width / 2;
    const isTop = mouseY < height / 2;

    const offsetPx = 10;
    const extraPx = 5;
    return {
      position: "absolute",
      pointerEvents: "none",
      // Above the brush overlay, so a persisted brush does not tint the tooltip.
      zIndex: 2,
      ...(isTop ? { top: mouseY + offsetPx } : { bottom: height - mouseY + offsetPx + extraPx }),
      ...(isLeft ? { left: mouseX + offsetPx + extraPx } : { right: width - mouseX + offsetPx }),
    };
  }, [hoverInfo, width, height]);

  return (
    <>
      <div
        ref={containerRef}
        style={{
          width, height, position: "relative", backgroundColor,
          // Long-clicking to start a brush otherwise selects surrounding text.
          userSelect: enableBrushCreate ? "none" : undefined,
        }}
      >
        {!supportsWebGpu ? (
          <p>{supportsWebGpuMessage}</p>
        ) : null}
        <div
          ref={cameraElementRef}
          style={{
            position: "absolute",
            top: marginTop,
            left: marginLeft,
            width: width - marginLeft - marginRight,
            height: height - marginTop - marginBottom,
            border: `${debugMargins ? 1 : 0}px solid red`,
          }}
        />
        {bailedEarly || cameraMatrix === undefined ? (
          <progress
            id={progressBarId}
            aria-label="Loading..."
            style={{
              bottom: 0,
              left: 0,
              width: '100%',
              position: 'absolute'
            }}
          />
        ) : null}
        {isVector ? (
          <svg
            ref={svgRef}
            style={{ width, height, border: `${debugMargins ? 1 : 0}px solid black` }}
            width={width}
            height={height}
            viewBox={`0 0 ${width} ${height}`}
            xmlns="http://www.w3.org/2000/svg"
            {...(bailedEarly ? ({
              ['aria-busy']: true,
              ['aria-describedby']: progressBarId,
            }) : {})}
          >
          </svg>
        ) : (
          <canvas
            ref={canvasRef}
            style={{ width, height, border: `${debugMargins ? 1 : 0}px solid black` }}
            width={width}
            height={height}
            {...(bailedEarly ? ({
              ['aria-busy']: true,
              ['aria-describedby']: progressBarId,
            }) : {})}

          />
        )}
        <BrushOverlay
          width={width}
          height={height}
          overlayRef={brushOverlayRef}
          geometry={brushGeometry}
          color={brushColor}
          brushState={brushState}
          pressProgress={pressProgress}
          isBrushHovered={isBrushHovered}
          enableBrushEdit={enableBrushEdit}
          onVertexMouseDown={onVertexMouseDown}
          onEdgeMouseDown={onEdgeMouseDown}
          onClearClick={onClearClick}
        />
        {hoverInfo ? (
          <div style={hoverStyle ?? undefined}>
            <Tooltip content={hoverInfo.content} asTable />
          </div>
        ) : null}
      </div>
      <button ref={tempButtonRef} style={{ display: 'none' }}>Try lookAt</button>
    </>
  );
}

// `PluotInner` uses react-query internally to drive rendering, so it needs a
// `QueryClientProvider` ancestor; consumers shouldn't have to set one up
// themselves, so each `Pluot` instance brings its own isolated `QueryClient`.
export function Pluot(props: PluotProps) {
  const [queryClient] = useState(() => new QueryClient());
  return (
    <QueryClientProvider client={queryClient}>
      <PluotInner {...props} />
    </QueryClientProvider>
  );
}
