import React, { useLayoutEffect, useEffect, useEffectEvent, useRef, useState, useMemo, useReducer, useCallback } from "react";
import { mat4, vec4 } from "gl-matrix";
import lzs from "lz-string";
import { isEqual, throttle } from "lodash-es";
import { FetchStore } from 'zarrita';
import {
  initialize, getIsWasmReady,
  render_wasm, pick_wasm,
  setStore, getStore,
  storeInstanceToMetadata,
  getBounds, getCameraMatrixFromBounds,
  checkWebGpuFeatureDetection,
  onMouseMove2d, onWheel2d,
  onMouseMove3d, onWheel3d,
} from '@pluot/core';

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

function normalizePickingResult(data) {
  const result = data;
  if (data && Array.isArray(result.layer_results)) {
    result.layer_results = result.layer_results.map(obj => ({
      layer_id: obj.layer_id,
      // This is needed because serde-wasm-bindgen
      // converts Rust HashMap to JS Map.
      info: Object.fromEntries(Array.from(obj.info)),
    }));
  }
  return result;
}


export function Pluot(props) {
  const {
    width: widthProp,
    height: heightProp,
    plotId,
    plotType,
    store,
    storeName: storeNameProp,
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
    cameraMatrix: controlledCameraMatrix = null,
    setCameraMatrix: setControlledCameraMatrix = null,
    enablePicking = true,
    backgroundColor = undefined,
  } = props;

  // If cameraMatrix is not provided, then we manage the camera matrix internally.
  const [uncontrolledCameraMatrix, setUncontrolledCameraMatrix] = useState(
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
  const setCameraMatrix = isControlledCamera
    ? setControlledCameraMatrix
    : setUncontrolledCameraMatrix;

  const width = Math.floor(widthProp);
  const height = Math.floor(heightProp);

  const isVector = format === "Vector";

  const storeName = useMemo(() => {
    if (storeNameProp) {
      return storeNameProp;
    }
    // If store is a string, assume it is a URL and initialize a FetchStore here.
    if (store) {
      if (typeof store === 'string') {
        return setStore(new FetchStore(store), plotId);
      }
      return setStore(store, plotId);
    }
    throw new Error("Either storeName or store must be provided.");
  }, [storeNameProp, store]);

  // Build the top-level `stores` map that RenderParams expects: a mapping from
  // store name to its derived `ZarrStoreInfo` metadata. Store instances were
  // registered by name (via setStore/setStoreByName); here we derive each one's
  // portable metadata from the registered instance. We collect every store
  // referenced by the layers (via their `store_name`) plus the default
  // `storeName`, so that multi-store plots resolve correctly.
  const stores = useMemo(() => {
    const names = new Set();
    if (storeName) names.add(storeName);
    for (const layer of plotParams?.layers ?? []) {
      const layerStoreName = layer?.layer_params?.store_name;
      if (layerStoreName) names.add(layerStoreName);
    }
    const result = {};
    for (const name of names) {
      const instance = getStore(name);
      if (instance) result[name] = storeInstanceToMetadata(instance);
    }
    return Object.keys(result).length > 0 ? result : undefined;
  }, [storeName, plotParams]);

  const [supportsWebGpu, supportsWebGpuMessage] = useMemo(checkWebGpuFeatureDetection, []);

  const svgRef = useRef(null);
  const canvasRef = useRef(null);
  const cameraElementRef = useRef(null);

  const tempButtonRef = useRef(null);

  // We may want to update these things without triggering a re-render.
  const isRenderingRef = useRef(false);
  const currentTimeout = useRef(minTimeout);

  // TODO: do we want to use the backlog approach or not?
  // (Similar to the one used in the Vitessce heatmap)
  // Reference: https://github.com/vitessce/vitessce/blob/71f17fb605768e0428fb15ed87b3ea34bcbb4803/packages/view-types/heatmap/src/Heatmap.js#L368
  //const backlogRef = useRef([]);
  const [backlogIteration, incBacklogIteration] = useReducer(i => i + 1, 0);

  const [isWasmReady, setIsWasmReady] = useState(false);
  const [didFirstRender, setDidFirstRender] = useState(false);
  const [bailedEarly, setBailedEarly] = useState(true);

  const [pickingResult, setPickingResult] = useState(null);
  const [scriptResult, setScriptResult] = useState(null);

  useLayoutEffect(() => {
    initialize().then(() => setIsWasmReady(getIsWasmReady()));
  }, []);


  const wheelHandler = useEffectEvent((event) => {
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

  const mouseMoveHandler = useEffectEvent((event) => {
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

  // The picking callback.
  const pickFrame = useEffectEvent(async (screenCoordX, screenCoordY) => {
    const renderParams = {
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

    setPickingResult(normalizePickingResult(await pick_wasm(
      renderParams,
      // The coordinates are relative to the "layer" (the camera region), not the full width/height.
      // We also need to flip the Y coordinate so that positive is up.
      screenCoordX + marginLeft,
      marginBottom + (layerHeight - screenCoordY)
    )));
  });

  // Set up the camera and picking handlers.
  useEffect(() => {
    const cameraEl = cameraElementRef.current;

    if (!cameraEl) {
      return () => {};
    }

    // Create a 2D camera for handling zoom and pan.
    cameraEl.addEventListener("mousemove", mouseMoveHandler);
    cameraEl.addEventListener("wheel", wheelHandler);

    // Set up an onClick handler for picking.
    const clickHandler = (event) => {
      if (enablePicking) {
        pickFrame(event.offsetX, event.offsetY);
      }
    };
    cameraEl.addEventListener("click", clickHandler);

    return () => {
      cameraEl.removeEventListener("mousemove", mouseMoveHandler);
      cameraEl.removeEventListener("wheel", wheelHandler);
      cameraEl.removeEventListener("click", clickHandler);
    };
  }, [viewMode, enablePicking]);


  const renderToScript = useEffectEvent(async () => {
    isRenderingRef.current = true;

    const renderParams = {
      width,
      height,
      format: "ScriptRust",
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
    let arr;
    try {
      arr = await render_wasm(renderParams);

      isRenderingRef.current = false;
    } catch (error) {
      console.error("Error during wasm.render_wasm (rendering to script):", error);
      // Cleanup
      isRenderingRef.current = false;
      return;
    }

    const scriptContents = (new TextDecoder()).decode(arr);
    console.log(scriptContents);

  });


  // The renderFrame callback.
  // We use useEffectEvent because we want to "see"
  // the latest values of viewMatrix, plotProps, etc.
  const renderFrame = useEffectEvent(async () => {
    isRenderingRef.current = true;
    console.log('wasm.render');

    const renderParams = {
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
    let arr;
    try {
      arr = await render_wasm(renderParams);

      isRenderingRef.current = false;
    } catch (error) {
      console.error("Error during wasm.render_wasm:", error);
      // Cleanup
      isRenderingRef.current = false;
      return;
    }

    if (isVector) {
      // Format: Vector (render to SVG)
      const gContents = decompressFromUint8Array(arr);

      //console.log(gContents)

      if (!svgRef.current) {
        return;
      }
      svgRef.current.innerHTML = gContents;

      // TODO: check for bailed early
    } else {
      // Format: Raster (render to canvas)
      const canvas = canvasRef.current;
      if (!canvas) {
        return;
      }
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        return;
      }
      // TODO: is there a more efficient way to do this?
      // E.g., write to a webgl texture? or is this fast enough already?
      const imageData = new ImageData(
        new Uint8ClampedArray(arr.subarray(0, -1)),
        width,
        height,
      );
      ctx.putImageData(imageData, 0, 0);

      const frameBailedEarly = arr.at(-1) === 1;
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
        const storeUsed = getStore(storeName);
        if (storeUsed && storeUsed.clearCache && typeof storeUsed.clearCache === 'function') {
          storeUsed.clearCache();
        }
      }
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
  }, [plotId, plotType, plotParams, storeName, format,
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
  }, [isWasmReady, didFirstRender, cameraMatrix, backlogIteration, plotId, plotType, plotParams, storeName, format,
    width, height, aspectRatioMode, aspectRatioAlignmentMode, marginLeft, marginRight, marginTop, marginBottom]);

  return (
    <>
      <div style={{ width, height, position: "relative", backgroundColor }}>
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
        {isVector ? (
          <svg
            ref={svgRef}
            style={{ width, height, border: `${debugMargins ? 1 : 0}px solid black` }}
            width={width}
            height={height}
            viewBox={`0 0 ${width} ${height}`}
            xmlns="http://www.w3.org/2000/svg"
          >
          </svg>
        ) : (
          <canvas
            ref={canvasRef}
            style={{ width, height, border: `${debugMargins ? 1 : 0}px solid black` }}
            width={width}
            height={height}
          />
        )}
      </div>
      {bailedEarly ? (
          <p>Loading...</p>
        ) : null}
      <button ref={tempButtonRef} style={{ display: 'none' }}>Try lookAt</button>
      {pickingResult ? (
        <pre>{JSON.stringify(pickingResult, null, 2)}</pre>
      ) : null}
      <button onClick={renderToScript}>Render to script</button>
      {scriptResult ? (
        <pre>{scriptResult}</pre>
      ) : null}
    </>
  );
}
