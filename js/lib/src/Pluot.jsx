// TODO: once things are working with react,
// convert to use plain vanilla JS.
import React, { useLayoutEffect, useEffect, useRef, useState } from 'react';
import * as wasm from 'pluot';
import { FetchStore } from 'zarrita';
import { select as d3_select } from 'd3-selection';
import { zoom as d3_zoom, zoomIdentity as d3_zoomIdentity } from 'd3-zoom';
import createDom2dCamera from "dom-2d-camera";
import { mat4, vec4 } from 'gl-matrix';
import { lru } from "./lru-store.js";

const DEFAULT_VIEW = new Float32Array([
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1, 0,
  0, 0, 0, 1,
]);

//const baseUrl = 'https://storage.googleapis.com/vitessce-demo-data/use-coordination/mnist.zarr';
const baseUrl = 'http://localhost:5173/@data/mnist.zarr';

const stores = {
    // TODO: wrap store in a cache.
    // See https://github.com/hms-dbmi/vizarr/blob/862745c1c7c095748bbe97475da61807d5b49189/src/utils.ts#L47
    'mnist_store': lru(new FetchStore('http://localhost:5173/@data/mnist.zarr')),
    'gaussian_quantiles_store': lru(new FetchStore('http://localhost:5173/@data/gaussian_quantiles.zarr')),
}

// console.log(wasm);

// Define the global zarr_get function.
// TODO: figure out how to pass into wasm.default as a parameter, rather than setting on window/globally.
window.zarr_get = async (store_name, key) => {
    console.log(`zarr_get: store_name=${store_name}, key=${key}`);
    return stores[store_name].get(`/${key}`);
};

window.zarr_has = async (store_name, key) => {
    // console.log(`zarr_has: store_name=${store_name}, key=${key}`);
    return stores[store_name].get(`/${key}`) !== undefined;
};

window.zarr_get_range_from_offset = async (store_name, key, offset, length) => {
    return stores[store_name].getRange(`/${key}`, { offset, length });
};
window.zarr_get_range_from_end = async (store_name, key, suffix_length) => {
    return stores[store_name].getRange(`/${key}`, { suffix_length });
};

// console.log(await stores['my_store'].get('/umap/x_coords/zarr.json'));

export function Pluot(props) {
    const {
        width,
        height,
        plotType = 'scatterplot',
        renderOnce = true,
        logPerformance = false,
    } = props;

    const canvasRef = useRef(null);
    const [isWasmReady, setIsWasmReady] = useState(false);

    const [zoom, setZoom] = useState(0.0);
    const [targetX, setTargetX] = useState(0.0);
    const [targetY, setTargetY] = useState(0.0);

    useLayoutEffect(() => {
        const initWasm = async () => {
            await wasm.default();
            await wasm.set_panic_hook();
            setIsWasmReady(true);
        };
        initWasm();
    }, []);

    useEffect(() => {
        // Set up the d3-zoom handler.
        const canvas = canvasRef.current;
        if (!canvas) {
            return;
        }

        function onCameraEvent(camera, event) {
            camera.tick();
            
            // Compute the plot's x and y range from the view matrix, though these could come from any source
            // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1648
            const s = (2 / (2 / camera.view[0])) * (2 / (2 / camera.view[5]));
            console.log(s, Math.log2(s));
            const nextZoom = Math.log2(s);
            setZoom(nextZoom);
            const nextTargetX = camera.target[0]*2;
            const nextTargetY = camera.target[1]*2;
            setTargetX(nextTargetX);
            setTargetY(nextTargetY);
        }

        // Create a 2D camera for handling zoom and pan.
        const camera = createDom2dCamera(canvas,{
            isFixed: false,
            distance: zoom,
            target: [targetX, targetY],
            defaultMouseDownMoveAction: 'pan',

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
        });
        camera.setView(mat4.clone(DEFAULT_VIEW));

        /*
        function zoomed(zoomEvent) {
            if(zoomEvent && zoomEvent.transform) {
                const scale = zoomEvent.transform.k;
                const translateX = zoomEvent.transform.x;
                const translateY = zoomEvent.transform.y;
                console.log('zoomed', zoomEvent.transform);
                // TODO: update the zoom state.
                setZoom(prevZoom => Math.log2(2**prevZoom - (scale-1)));
                setTargetX(prevTargetX => prevTargetX + (translateX / width));
                setTargetY(prevTargetY => prevTargetY + (translateY / height));
            }
        }

        const selection = d3_select(canvas);
        selection.call(
            d3_zoom()
                .on("zoom", zoomed)
        );

        zoomed(d3_zoomIdentity);
        */

        // 
    }, [canvasRef]);

    useEffect(() => {
        const canvas = canvasRef.current;
        if(!canvas || !isWasmReady) {
            return;
        }
        const ctx = canvas.getContext('2d');

        // Start FPS tracking variables.
        let frameCount = 0;
        let lastTime = performance.now();
        let fps = 0;
        // End FPS tracking variables.

        // Render once or every animation frame.
        // Define the function to render a single frame.
        function renderFrame() {
            // console.log('wasm.render');
            const renderParams = {
                width,
                height,
                zoom,
                targetX,
                targetY,
                plotId: 'my_plot',
                plotType,
                storeName: 'gaussian_quantiles_store',
            };
            wasm.render(renderParams).then(arr => {
                // TODO: is there a more efficient way to do this?
                // E.g., write to a webgl texture? or is this fast enough already?
                const imageData = new ImageData(new Uint8ClampedArray(arr), width, height);
                ctx.putImageData(imageData, 0, 0);
            });
        }
        function animate() {
            // Start FPS tracking logic.
            const currentTime = performance.now();
            frameCount++;
            
            // Calculate FPS every second
            if (currentTime - lastTime >= 1000) {
                // The division by 1000 converts the time difference from milliseconds to seconds.
                // E.g., If 60 frames were rendered in 1000ms: 60 / (1000 / 1000) = 60 FPS
                // E.g., If 30 frames were rendered in 500ms:  30 / (500  / 1000) = 60 FPS
                // E.g., If 45 frames were rendered in 1500ms: 45 / (1500 / 1000) = 30 FPS
                fps = (frameCount / ((currentTime - lastTime) / 1000));
                if(logPerformance) {
                    console.log(`Average FPS: ${fps}`);
                }
                frameCount = 0;
                lastTime = currentTime;
            }
            // End FPS tracking logic.

            renderFrame();
            requestAnimationFrame(animate);
        }

        // Initialize data and kick off the first render.
        if(renderOnce) {
            renderFrame();
        } else {
            requestAnimationFrame(animate);
        }
    }, [isWasmReady, zoom, targetX, targetY]);

    return (
        <>
            <div style={{ width, height }}>
                <canvas
                    ref={canvasRef}
                    style={{ width, height }}
                    width={width}
                    height={height}
                />
            </div>
            <input
                type="range"
                min={-5}
                max={5}
                step={0.01}
                value={zoom}
                onChange={(e) => {
                    const newValue = parseFloat(e.target.value);
                    setZoom(newValue);
                }}
            />
        </>
    );
}