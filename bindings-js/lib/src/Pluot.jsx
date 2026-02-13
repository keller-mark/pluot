// TODO: once things are working with react,
// implement a plain/vanilla JS version.
import React, { useLayoutEffect, useEffect, useRef, useState, useMemo, useReducer } from "react";
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
import { isEqual } from "lodash-es";

// Needed due to "SyntaxError: Named export 'decompressFromUint8Array' not found.
// The requested module 'lz-string' is a CommonJS module,
// which may not support all module.exports as named exports."
const { decompressFromUint8Array } = lzs;

const DEFAULT_VIEW = new Float32Array([
  1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1,
]);

//const baseUrl = 'https://storage.googleapis.com/vitessce-demo-data/use-coordination/mnist.zarr';
const baseUrl = "http://localhost:5173/@data/mnist.zarr";

// TODO: move store registration into demo subpackage and via props (rather than constructing stores in lib subpackage).
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
    return stores[store_name].getRange(`/${key}`, { suffix_length });
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
    minTimeout = 50,
    maxTimeout = 500,
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
  const [isWasmReady, setIsWasmReady] = useState(false);

  const [viewMatrix, setViewMatrix] = useState(
    // Note: We use an initializer function here to avoid
    // sharing the same Float32Array among multiple Pluot
    // component instances that may be rendered on the same page.
    () => new Float32Array(DEFAULT_VIEW)
  );
  const viewMatrixRef = useRef(new Float32Array(DEFAULT_VIEW));

  const [isRendering, setIsRendering] = useState(false);
  const [didFirstRender, setDidFirstRender] = useState(false);

  // We keep a backlog of render param settings here.
  const backlogRef = useRef([]);
  const isRenderingRef = useRef(false);
  const currentTimeout = useRef(maxTimeout);

  const [cameraIteration, incCameraIteration] = useReducer(i => i + 1, 0);
  const [backlogIteration, incBacklogIteration] = useReducer(i => i + 1, 0);

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
        /*
        setViewMatrix(prev => {
          // Since camera events happen even on mousemove events that do not change the matrix,
          // we check for equality here to avoid unnecessary state updates and plot re-renders.
          if (isEqual(prev, camera.view)) {
            return prev;
          }
          return mat4.clone(camera.view)
        });
        */

        if (!isEqual(viewMatrixRef.current, camera.view)) {
          viewMatrixRef.current = mat4.clone(camera.view);
          /*
          const latestBacklogItem = backlogRef.current.pop();
          if (latestBacklogItem) {
            backlogRef.current.push({
              ...latestBacklogItem,
              camera_view: viewMatrixRef.current,
            });
          }
          */
        }
        // The user is interacting, so we reduce the timeout to improve responsiveness.
        currentTimeout.current = minTimeout;
        incCameraIteration();
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
      camera.setView(viewMatrixRef.current);
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
    //setViewMatrix(new Float32Array(DEFAULT_VIEW));
    viewMatrixRef.current = new Float32Array(DEFAULT_VIEW);
  }, [plotId]);

  // TODO: switch this useEffect to use React-Query.
  useEffect(() => {
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
      //zoom, // No longer used
      //targetX, // No longer used
      //targetY, // No longer used
      camera_view: viewMatrixRef.current,
      plot_id: plotId,
      plot_type: plotType,
      store_name: storeName,
      plot_params: plotParams,
      // Reduce the timeout value to improve responsiveness during data loading (bailed-early renders).
      timeout: 200, // in ms
      cache_enabled: true,
      svg_compression_enabled: true,
    };

    // TODO: should backlog be a plain state variable instead of a ref?
    backlogRef.current.push(renderParams);
    incBacklogIteration();
  }, [isWasmReady, plotId, plotType, plotParams, storeName, format, isVector, cameraIteration]);

  useEffect(() => {
    if (backlogRef.current.length === 0) {
      return;
    }

    if (!isWasmReady) {
      return;
    }

    // TODO: use react-query to manage the backlog of async render calls?
    // Alternatively, implement a backlog similar to the one used in the Vitessce heatmap.
    // Reference: https://github.com/vitessce/vitessce/blob/71f17fb605768e0428fb15ed87b3ea34bcbb4803/packages/view-types/heatmap/src/Heatmap.js#L368
    if (isRenderingRef.current) {
      // Prevent multiple render calls prior to the first successful render.
      return;
    }

    // Pop the most recent render params from the backlog.
    const latestRenderParams = backlogRef.current.pop();

    // Render once or every animation frame.
    // Define the function to render a single frame.
   async function renderFrame(renderParams) {
     isRenderingRef.current = true;
      // console.log('wasm.render');

      // TODO: wrap render_wasm in try/catch, to handle Rust panics.
      let arr;
      try {
        arr = await wasm.render_wasm({
          ...renderParams,
          camera_view: viewMatrixRef.current,
          timeout: currentTimeout.current,
        });
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

        const bailedEarly = arr.at(-1) === 1;
        if (bailedEarly) {
          // TODO: prevent infinite loop if always bailing early?
          backlogRef.current.push(renderParams);
          incBacklogIteration();
        } else {
          // Successful render.
          // Insert latest renderParams into backlog, without incrementing iteration.
          backlogRef.current = [];
          currentTimeout.current = maxTimeout;

          // TODO: clear the LRU cache for the store (via its store_name) corresponding to the rendered plot.
          setDidFirstRender(true);
        }
      }

      isRenderingRef.current = false;
    }

    // Call the async render_wasm function.
    renderFrame(latestRenderParams);
  }, [backlogIteration]);

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
      {!didFirstRender ? (
          <p>Loading...</p>
        ) : null}
    </>
  );
}
