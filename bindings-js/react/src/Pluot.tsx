import React, { useLayoutEffect, useEffect, useEffectEvent, useRef, useState, useMemo, useReducer, useId, type CSSProperties } from "react";
import lzs from "lz-string";
import { throttle } from "lodash-es";
import {
  initialize, getIsWasmReady,
  render_wasm, pick_wasm,
  normalizeStores, getStore,
  checkWebGpuFeatureDetection,
  onMouseMove2d, onWheel2d,
  onMouseMove3d, onWheel3d,
  type CameraMatrix,
} from '@pluot/core';
import { Tooltip } from "./Tooltip.js";
import { BrushOverlay } from "./BrushOverlay.js";
import { useBrush } from "./useBrush.js";
import type {
  HoverInfo, PickingResult, PluotProps, RawPickingResult, RenderParams, TooltipContent,
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


export function Pluot(props: PluotProps) {
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
    allowSimultaneousRenders = true,
    debugMargins = false,
    backgroundColor = undefined,
    cameraMatrix: controlledCameraMatrix = null,
    setCameraMatrix: setControlledCameraMatrix = null,
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
    brush = null,
    onBrush,
    onBrushEnd,
    onBrushClear,
  } = props;

  const onClick: (result: PickingResult) => void = typeof onClickProp === 'function' ? onClickProp : noop;
  const onHover: (result: PickingResult) => TooltipContent = typeof onHoverProp === 'function' ? onHoverProp : identity;



  // If cameraMatrix is not provided, then we manage the camera matrix internally.
  const [uncontrolledCameraMatrix, setUncontrolledCameraMatrix] = useState<CameraMatrix>(
    // Note: We use an initializer function here to avoid
    // sharing the same Float32Array among multiple Pluot
    // component instances that may be rendered on the same page.
    () => Float32Array.from(
      // If the cameraMatrix prop was provided, use that for the initial camera matrix;
      // otherwise use the default matrix.
      controlledCameraMatrix === null
        ? (viewMode === "2d" ? DEFAULT_VIEW : DEFAULT_3D_VIEW)
        : controlledCameraMatrix
    )
  );

  // Decide which camera matrix and setter to use.
  // If the user provides the cameraMatrix prop but NOT the setCameraMatrix setter,
  // then interpret the prop as the "initial" camera settings, but still treat as uncontrolled.
  const isControlledCamera = typeof setControlledCameraMatrix === "function";
  // Alternatively, if the user provides the setCameraMatrix setter, but NOT
  // the cameraMatrix, interpret this as they want to use the default camera
  // value initially, but they still want a controlled camera matrix.
  const cameraMatrix = isControlledCamera && controlledCameraMatrix !== null
    ? controlledCameraMatrix
    : uncontrolledCameraMatrix;
  const setCameraMatrix: (nextCameraMatrix: CameraMatrix) => void = isControlledCamera
    ? setControlledCameraMatrix
    : setUncontrolledCameraMatrix;

  const width = Math.floor(widthProp);
  const height = Math.floor(heightProp);

  const isVector = format === "Vector";

  // Build the top-level `stores` map that RenderParams expects: a mapping from
  // store name to its derived `ZarrStoreInfo` metadata.
  const stores = useMemo(() => normalizeStores({
    stores: storesProp,
    store: storeProp,
    storeName: storeNameProp,
    plotId,
    register: registerStores,
  }), [storeNameProp, storeProp, storesProp, plotId, registerStores]);

  const [supportsWebGpu, supportsWebGpuMessage] = useMemo(checkWebGpuFeatureDetection, []);

  const svgRef = useRef<SVGSVGElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const cameraElementRef = useRef<HTMLDivElement | null>(null);
  // The outer (width x height) element, which is the coordinate space that both
  // the brush overlay and the hover tooltip are positioned within.
  const containerRef = useRef<HTMLDivElement | null>(null);

  const tempButtonRef = useRef<HTMLButtonElement | null>(null);

  // We may want to update these things without triggering a re-render.
  const isRenderingRef = useRef(false);
  const currentTimeout = useRef(minTimeout);

  // Used to distinguish a plain click from a click that ends a drag
  // (e.g. panning), so that dragging does not trigger picking.
  const dragStartRef = useRef<{ x: number, y: number } | null>(null);
  const didDragRef = useRef(false);

  // TODO: do we want to use the backlog approach or not?
  // (Similar to the one used in the Vitessce heatmap)
  // Reference: https://github.com/vitessce/vitessce/blob/71f17fb605768e0428fb15ed87b3ea34bcbb4803/packages/view-types/heatmap/src/Heatmap.js#L368
  //const backlogRef = useRef([]);
  const [backlogIteration, incBacklogIteration] = useReducer((i: number) => i + 1, 0);

  const [isWasmReady, setIsWasmReady] = useState(false);
  const [didFirstRender, setDidFirstRender] = useState(false);
  const [bailedEarly, setBailedEarly] = useState(true);

  // hoverInfo.mouseX/mouseY are in the coordinate space of the outer
  // (width x height) container, used to position the hover tooltip.
  const [hoverInfo, setHoverInfo] = useState<HoverInfo | null>(null);

  const progressBarId = useId();

  const {
    brushState,
    pressProgress,
    isBrushHovered,
    isBrushingRef,
    shouldSuppressClickRef,
    onVertexMouseDown,
    onClearClick,
  } = useBrush({
    containerRef,
    width, height,
    marginTop, marginRight, marginBottom, marginLeft,
    aspectRatioMode, aspectRatioAlignmentMode, cameraMatrix,
    brushUnitsModeX, brushUnitsModeY,
    brushMarginTop, brushMarginRight, brushMarginBottom, brushMarginLeft,
    enableBrushCreate, enableBrushEdit, enableBrushClear,
    brushDelay, maybeBrushDelay, persistBrush, brushMode,
    brush, onBrush, onBrushEnd, onBrushClear,
  });

  useLayoutEffect(() => {
    initialize().then(() => setIsWasmReady(getIsWasmReady()));
  }, []);

  const wheelHandler = useEffectEvent((event: WheelEvent) => {
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

  // Runs the picking query against the wasm module and returns the normalized result.
  // Shared by the click (pickFrame) and hover (hoverFrame) callbacks below.
  const pick = useEffectEvent(async (screenCoordX: number, screenCoordY: number): Promise<PickingResult> => {
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
      // Should see the latest viewMatrix here, since renderFrame is wrapped in useEffectEvent.
      camera_view: cameraMatrix,
      plot_id: plotId,
      plot_type: plotType,
      stores,
      plot_params: plotParams,
      // Reduce the timeout value to improve responsiveness during data loading (bailed-early renders)?
      timeout: currentTimeout.current, // in ms // Note: will not have any effect when wait_for_store_gets is false.
      wait_for_store_gets: false, // TODO: lift this value up to pass/use it in the window.zarr_ functions as well?
      cache_enabled: true,
      svg_compression_enabled: true,
      svg_include_document: false,
    };

    const layerHeight = height - marginTop - marginBottom;

    // TODO: wrap pick_wasm in a try/catch

    return normalizePickingResult(await pick_wasm(
      renderParams,
      // The coordinates are relative to the "layer" (the camera region), not the full width/height.
      // We also need to flip the Y coordinate so that positive is up.
      screenCoordX + marginLeft,
      marginBottom + (layerHeight - screenCoordY)
    ));
  });

  // The click-picking callback.
  const pickFrame = useEffectEvent(async (screenCoordX: number, screenCoordY: number) => {
    onClick(await pick(screenCoordX, screenCoordY));
  });

  // The hover-picking callback.
  const hoverFrame = useEffectEvent(async (screenCoordX: number, screenCoordY: number) => {
    const result = await pick(screenCoordX, screenCoordY);
    setHoverInfo({
      content: onHover(result),
      // Convert from cameraEl-relative coordinates to outer-container-relative
      // coordinates, since the tooltip is positioned within the outer container.
      mouseX: screenCoordX + marginLeft,
      mouseY: screenCoordY + marginTop,
    });
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


  // The renderFrame callback.
  // We use useEffectEvent because we want to "see"
  // the latest values of viewMatrix, plotProps, etc.
  const renderFrame = useEffectEvent(async () => {
    isRenderingRef.current = true;
    console.log('wasm.render');

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
      // Should see the latest viewMatrix here, since renderFrame is wrapped in useEffectEvent.
      camera_view: cameraMatrix,
      plot_id: plotId,
      plot_type: plotType,
      stores,
      plot_params: plotParams,
      // Reduce the timeout value to improve responsiveness during data loading (bailed-early renders)?
      timeout: currentTimeout.current, // in ms // Note: will not have any effect when wait_for_store_gets is false.
      wait_for_store_gets: false, // TODO: lift this value up to pass/use it in the window.zarr_ functions as well?
      cache_enabled: true,
      svg_compression_enabled: true,
      svg_include_document: false,
    };

    // Wrap render_wasm in try/catch, to handle Rust panics.
    let arr: Uint8Array;
    try {
      arr = await render_wasm(renderParams);

      isRenderingRef.current = false;
    } catch (error) {
      console.error("Error during wasm.render_wasm:", error);
      // Cleanup
      isRenderingRef.current = false;
      return;
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
            width,
            height,
          );
          ctx.putImageData(imageData, 0, 0);
        }
      }
    }

    if (frameBailedEarly) {
      // We multiply the current timeout by two to implement an exponential backoff
      // while the Rust side is bailing early.
      // A downstream useEffect restarts the exponential backoff from scratch
      // if any other plotting parameters change.
      currentTimeout.current = Math.min(currentTimeout.current * 2, maxTimeout);
      incBacklogIteration(); // Increment this to force a re-render.
      setBailedEarly(true); // Update this to show the loading indicator.
    } else {
      // Successful render.
      currentTimeout.current = minTimeout;
      setBailedEarly(false); // Update this to hide the loading indicator.

      // Clear the LRU cache for the store (via its store_name) corresponding to the rendered plot.
      Object.keys(stores ?? {}).forEach(storeName => {
        const storeUsed = getStore(storeName);
        if (storeUsed && typeof storeUsed.clearCache === 'function') {
          storeUsed.clearCache();
        }
      });

    }

    setDidFirstRender(true);
  });


  const throttledRender = useMemo(
    () => throttle(
      renderFrame,
      16, // ~60fps
      // When both leading and trailing are true (the default):
      // - First call -> executes immediately (leading edge)
      // - Calls during the wait window -> ignored, but the most recent one is remembered.
      // - After the wait period expires -> the last remembered call is executed (trailing edge).
      { leading: true, trailing: true }
    ), []);

  useEffect(() => {
    return () => throttledRender.cancel();
  }, [throttledRender]);

  // Reset the backoff timeout whenever plot parameters change so the next
  // sequence of bailed-early renders starts from the minimum again.
  useEffect(() => {
    currentTimeout.current = minTimeout;
  }, [plotId, plotType, plotParams, stores, format,
    width, height, aspectRatioMode, aspectRatioAlignmentMode,
    marginLeft, marginRight, marginTop, marginBottom,
    cameraMatrix,
  ]);

  // TODO: use react-query?
  useEffect(() => {
    if (!isWasmReady) {
      return;
    }

    // We want to allow for simultaneous renders, as this makes user interactions feel
    // much smoother. However, we allow for users to opt-out, and we also
    // need to prevent simultaneous renders prior to the first render, as the first
    // render initializes cached values and stuff.
    if (isRenderingRef.current && (!didFirstRender || bailedEarly || !allowSimultaneousRenders)) {
      // Prevent multiple render calls prior to the first successful render.
      return;
    }

    // Render on the next animation frame.
    throttledRender();
  }, [isWasmReady, didFirstRender, cameraMatrix, backlogIteration, plotId, plotType, plotParams, stores, format,
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
        {bailedEarly ? (
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
          brushState={brushState}
          pressProgress={pressProgress}
          isBrushHovered={isBrushHovered}
          enableBrushEdit={enableBrushEdit}
          onVertexMouseDown={onVertexMouseDown}
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
