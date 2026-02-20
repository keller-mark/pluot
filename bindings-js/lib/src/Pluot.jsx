// TODO: once things are working with react,
// implement a plain/vanilla JS version.
import React, { useLayoutEffect, useEffect, useEffectEvent, useRef, useState, useMemo, useReducer, useCallback } from "react";
import * as wasm from "pluot";
import { FetchStore } from "zarrita";
// import createDom2dCamera from "dom-2d-camera";
import createDom2dCamera from "./dom-2d-camera.js"; // Copy with minor modifications.
// import createCamera from "3d-view-controls";
import createCamera from "./3d-view-controls.js"; // Copy with minor modifications.
import { mat4, vec4 } from "gl-matrix";
import { lru } from "./lru-store.js";
import { useWebGpuFeatureDetection } from "./feature-detection.js";
import lzs from "lz-string";
import { isEqual, throttle } from "lodash-es";

// Needed due to "SyntaxError: Named export 'decompressFromUint8Array' not found.
// The requested module 'lz-string' is a CommonJS module,
// which may not support all module.exports as named exports."
const { decompressFromUint8Array } = lzs;

const DEFAULT_VIEW = new Float32Array([
  1/200, 0, 0, 0,
  0, 1/200, 0, 0,
  0, 0, 1/200, 0,
  0, 0, 0, 1,
]);

//const baseUrl = 'https://storage.googleapis.com/vitessce-demo-data/use-coordination/mnist.zarr';
const baseUrl = "http://localhost:5173/@data/mnist.zarr";

// TODO: remove these hard-coded stores once things are working, and only allow providing via props.
// (We will still need the global `stores` object so that the stores are available to the window.zarr_ functions.)
const stores = {
  // Wrap store in a cache.
  // See https://github.com/hms-dbmi/vizarr/blob/862745c1c7c095748bbe97475da61807d5b49189/src/utils.ts#L47
  mnist_store: lru(new FetchStore("http://localhost:5173/@data/mnist.zarr")),
  // NOTE: no longer using the lru cache to reduce memory usage,
  // since we now have the use_memo_ functions in cache.rs on the rust side.
  // Note: when using a timeout parameter, we still may want to use a cache
  // for in-progress promises (but not for their returned data).

  // Usage of lru cache results in increased memory usage, since data is stored both in the lru cache and in the wasm cache (via use_memo_ functions in cache.rs).
  // However, it is necessary especially when the network is slow, since it caches the promises that are re-requested from Rust on every re-render when bailing early due to the timeout parameter.
  // TODO: consider eviction based on time (N*timeout) following promise resolution, or upon being notified by Rust that it has successfully cached a particular key.
  // Or, evict following some N successful .get()s after promise resolution, to ensure that Rust has successfully accessed and cached the data while allowing for some timeouts-during-gets (though hopefully unlikely).
  // Alternatively, have Rust allocate a fixed-size cache, and store data there when promises resolve, caching/returning to Rust only pointers/lengths into this shared memory.
  gaussian_quantiles_store: lru(new FetchStore("http://localhost:5173/@data/gaussian_quantiles.zarr")),
  gaussian_quantiles_store_compressed: new FetchStore("http://localhost:5173/@data/gaussian_quantiles_compressed.zarr"),
  ome_ngff: lru(
    new FetchStore("http://localhost:5173/@data/6001240_labels.ome.zarr"),
  ),
  wheat: lru(
    new FetchStore("http://localhost:5173/@data/wheat.zarr"),
  ),
  ome_ngff_2: lru(
    new FetchStore('https://uk1s3.embassy.ebi.ac.uk/idr/zarr/v0.5/idr0157/Asterella%20gracilis%20SWE/IMG_1033-1112%20Asterella%20gracilis%20(Mannia%20gracilis)%20stature.ome.zarr')
  )
};

// Only use window if it is defined (i.e., in the browser).
// TODO: figure out how to pass into wasm.default as a parameter, rather than setting on window/globally.
if (typeof window !== 'undefined') {
  // Define the global zarr_get function.
  window.zarr_get = async (store_name, key) => {
    console.log(`zarr_get: store_name=${store_name}, key=${key}`);
    return stores[store_name].get(`/${key}`);
  };

  window.zarr_has = async (store_name, key) => {
    // console.log(`zarr_has: store_name=${store_name}, key=${key}`);
    return stores[store_name].get(`/${key}`) !== undefined;
  };

  window.zarr_get_range_from_offset = async (store_name, key, offset, length) => {
    // console.log(`zarr_get_range_from_offset: store_name=${store_name}, key=${key}, offset=${offset}, length=${length}`);
    return stores[store_name].getRange(`/${key}`, { offset, length });
  };
  window.zarr_get_range_from_end = async (store_name, key, suffix_length) => {
    // console.log(`zarr_get_range_from_end: store_name=${store_name}, key=${key}, suffix_length=${suffix_length}`);
    return stores[store_name].getRange(`/${key}`, { suffixLength: suffix_length });
  };

  window.isPluotInitialized = null;
}

export function Pluot(props) {
  const {
    width,
    height,
    plotId,
    plotType,
    store,
    storeName: storeNameProp,
    plotParams,
    renderOnce = true,
    logPerformance = false,
    mode = "2d",
    marginBottom = 100.0,
    marginLeft = 100.0,
    marginTop = 100.0,
    marginRight =  100.0,
    aspectRatioMode = "Contain", // "Ignore", "Contain", "Cover"
    format = "Raster", // "Raster", "Vector"
    minTimeout = 32,
    maxTimeout = 128,
    allowSimultaneousRenders = true,
  } = props;

  const isVector = format === "Vector";

  const storeName = useMemo(() => {
    if (storeNameProp) {
      return storeNameProp;
    }
    if (store) {
      stores[plotId + "_store"] = lru(store);
      return plotId + "_store";
    }
    throw new Error("Either storeName or store must be provided.");
  }, [storeNameProp, store]);

  const { supportsWebGpu, supportsWebGpuMessage } = useWebGpuFeatureDetection();

  const svgRef = useRef(null);
  const canvasRef = useRef(null);
  const cameraRef = useRef(null);

  // We may want to update these things without triggering a re-render.
  const isRenderingRef = useRef(false);
  const currentTimeout = useRef(maxTimeout);

  // TODO: do we want to use the backlog approach or not?
  // (Similar to the one used in the Vitessce heatmap)
  // Reference: https://github.com/vitessce/vitessce/blob/71f17fb605768e0428fb15ed87b3ea34bcbb4803/packages/view-types/heatmap/src/Heatmap.js#L368
  //const backlogRef = useRef([]);
  const [backlogIteration, incBacklogIteration] = useReducer(i => i + 1, 0);

  const [isWasmReady, setIsWasmReady] = useState(false);
  const [didFirstRender, setDidFirstRender] = useState(false);
  const [bailedEarly, setBailedEarly] = useState(true);

  // TODO: handle a viewMatrix that is provided and set via props,
  // to enable usage as a controlled component
  // (e.g., for linked views with shared cameras).
  const [viewMatrix, setViewMatrix] = useState(
    // Note: We use an initializer function here to avoid
    // sharing the same Float32Array among multiple Pluot
    // component instances that may be rendered on the same page.
    () => new Float32Array(DEFAULT_VIEW)
  );

  useLayoutEffect(() => {
    const initWasm = async () => {
      await wasm.default();
      await wasm.set_panic_hook();
    };
    if(!window.isPluotInitialized) {
      window.isPluotInitialized = initWasm().then(() => setIsWasmReady(true));
    } else {
      window.isPluotInitialized.then(() => setIsWasmReady(true));
    }
  }, []);


  // Set up the camera.
  useEffect(() => {
    // Set up the camera.
    const cameraEl = cameraRef.current;
    if (!cameraEl) {
      return;
    }

    let dispose = () => {};

    // Create a 2D camera for handling zoom and pan.
    if (mode === "2d") {
      function onCameraEvent(camera, event) {
        camera.tick();
        // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1648

        setViewMatrix(prev => {
          // Since camera events happen even on mousemove events that do not change the matrix,
          // we check for equality here to avoid unnecessary state updates and plot re-renders.
          if (isEqual(prev, camera.view)) {
            return prev;
          }
          return mat4.clone(camera.view)
        });

        currentTimeout.current = minTimeout;
      }

      const camera = createDom2dCamera(cameraEl, {
        isFixed: false,
        distance: 0.0,
        //target: [0.0, 0.0],
        //viewCenter: [0.5, 0.5], // Should this be used when the coordinate system is (0 to 1) rather than (-1 to 1)?
        viewCenter: [0.0, 0.0],
        defaultMouseDownMoveAction: "pan",

        onKeyDown: (event) => {
          onCameraEvent(camera, event);
        },
        onKeyUp: (event) => {
          onCameraEvent(camera, event);
        },
        onMouseDown: (event) => {
          onCameraEvent(camera, event);
        },
        onMouseUp: (event) => {
          onCameraEvent(camera, event);
        },
        onMouseMove: (event) => {
          onCameraEvent(camera, event);
        },
        onWheel: (event) => {
          onCameraEvent(camera, event);
        },
        aspectRatioMode: aspectRatioMode,
      });
      dispose = camera.dispose;

      // Set the initial view matrix.
      camera.setView(viewMatrix);
    } else if (mode === "3d") {
      function onCameraEvent(camera, event) {
        camera.tick();
        console.log(camera.matrix);
        setViewMatrix(mat4.clone(camera.matrix));
      }

      const camera = createCamera(cameraEl, {
        mode: "orbit",
        zoomSpeed: -3,
      });

      // TODO:
      // - fork 3d-view-controls and remove usage of "global" - then clean up vite config.
      // - define a camera.dispsose option.

      // Reference: https://github.com/flekschas/dom-2d-camera/blob/cd59ea035a0ea72c2c0535fa3721f8127946576c/src/index.js#L237C3-L315C71
      const keyUpHandler = (event) => {
        // TODO
      };

      const keyDownHandler = (event) => {
        // TODO
      };

      const mouseUpHandler = (event) => {
        // TODO
      };

      const mouseDownHandler = (event) => {
        // TODO
      };

      // TODO: use react state?
      var lastX = 0;
      var lastY = 0;

      // Reference: https://github.com/mikolalysenko/3d-view/blob/8269e02337bba1923173a750aa7f3f0f76c91ba5/example/minimal.js#L67
      const mouseMoveHandler = (event) => {
        /*
        var dx = (event.clientX - lastX) / width;
        var dy = -(event.clientY - lastY) / height;
        if (event.which === 1) {
          if (event.shiftKey) {
            //zoom
            camera.rotate(now(), 0, 0, dx);
          } else {
            //rotate
            camera.rotate(now(), dx, dy);
          }
        } else if (event.which === 3) {
          //pan
          camera.pan(now(), dx, dy);
        }
        lastX = event.clientX;
        lastY = event.clientY;
        */
        onCameraEvent(camera, event);
      };

      const wheelHandler = (event) => {
        //camera.pan(now(), 0, 0, event.deltaY);
        onCameraEvent(camera, event);
      };

      cameraEl.addEventListener("keydown", keyDownHandler);
      cameraEl.addEventListener("keyup", keyUpHandler);
      cameraEl.addEventListener("mousedown", mouseDownHandler);
      cameraEl.addEventListener("mouseup", mouseUpHandler);
      cameraEl.addEventListener("mousemove", mouseMoveHandler);
      cameraEl.addEventListener("wheel", wheelHandler);

      dispose = () => {
        cameraEl.removeEventListener("keydown", keyDownHandler);
        cameraEl.removeEventListener("keyup", keyUpHandler);
        cameraEl.removeEventListener("mousedown", mouseDownHandler);
        cameraEl.removeEventListener("mouseup", mouseUpHandler);
        cameraEl.removeEventListener("mousemove", mouseMoveHandler);
        cameraEl.removeEventListener("wheel", wheelHandler);
      };
    } else {
      throw new Error("Unknown mode found.");
    }

    return dispose;
  }, [cameraRef, mode, aspectRatioMode]);


  useEffect(() => {
    // Reset view matrix on plot change.
    // Create a new Float32Array to avoid sharing a mutable array
    // among multiple Pluot component instances.
    setViewMatrix(new Float32Array(DEFAULT_VIEW));
    //viewMatrixRef.current = new Float32Array(DEFAULT_VIEW);
  }, [plotId]);

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
      view_mode: "2d",
      pickable: false,
      // Should see the latest viewMatrix here, since renderFrame is wrapped in useEffectEvent.
      camera_view: viewMatrix,
      plot_id: plotId,
      plot_type: plotType,
      store_name: storeName,
      plot_params: plotParams,
      // Reduce the timeout value to improve responsiveness during data loading (bailed-early renders)?
      timeout: currentTimeout.current, // in ms
      cache_enabled: true,
      svg_compression_enabled: true,
    };

    // Wrap render_wasm in try/catch, to handle Rust panics.
    let arr;
    try {
      arr = await wasm.render_wasm(renderParams);
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
        currentTimeout.current = maxTimeout;
        incBacklogIteration(); // Increment this to force a re-render.
        setBailedEarly(true); // Update this to show the loading indicator.
      } else {
        // Successful render.
        setBailedEarly(false); // Update this to hide the loading indicator.

        // Clear the LRU cache for the store (via its store_name) corresponding to the rendered plot.
        const storeUsed = stores[renderParams.store_name];
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
  }, [isWasmReady, didFirstRender, viewMatrix, backlogIteration, plotId, plotType, plotParams, storeName, format]);

  return (
    <>
      <div style={{ width, height, position: "relative" }}>
        {!supportsWebGpu ? (
          <p>{supportsWebGpuMessage}</p>
        ) : null}
        <div
          ref={cameraRef}
          style={{
            position: "absolute",
            top: marginTop,
            left: marginLeft,
            width: width - marginLeft - marginRight,
            height: height - marginTop - marginBottom,
            border: "0px solid red",
          }}
        />
        {isVector ? (
          <svg
            ref={svgRef}
            style={{ width, height, border: "1px solid black" }}
            width={width}
            height={height}
            viewBox={`0 0 ${width} ${height}`}
            xmlns="http://www.w3.org/2000/svg"
          >
          </svg>
        ) : (
          <canvas
            ref={canvasRef}
            style={{ width, height, border: "1px solid black" }}
            width={width}
            height={height}
          />
        )}
      </div>
      {bailedEarly ? (
          <p>Loading...</p>
        ) : null}
    </>
  );
}
