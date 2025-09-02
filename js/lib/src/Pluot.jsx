// TODO: once things are working with react,
// convert to use plain vanilla JS.
import React, { useLayoutEffect, useEffect, useRef, useState } from 'react';
import * as wasm from 'pluot';
import { FetchStore } from 'zarrita';
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
    'ome_ngff': lru(new FetchStore('http://localhost:5173/@data/6001240_labels.ome.zarr')),
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
    // console.log(`zarr_get_range_from_offset: store_name=${store_name}, key=${key}, offset=${offset}, length=${length}`);
    return stores[store_name].getRange(`/${key}`, { offset, length });
};
window.zarr_get_range_from_end = async (store_name, key, suffix_length) => {
    // console.log(`zarr_get_range_from_end: store_name=${store_name}, key=${key}, suffix_length=${suffix_length}`);
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

    const [viewMatrix, setViewMatrix] = useState(DEFAULT_VIEW);
    
    /*
    const [zoom, setZoom] = useState(0.0);
    const [targetX, setTargetX] = useState(0.0);
    const [targetY, setTargetY] = useState(0.0);
    */
    const [pointRadius, setPointRadius] = useState(5.0);
    const [ch0Window, setCh0Window] = useState([0.0, 0.1]);
    const [ch1Window, setCh1Window] = useState([0.0, 0.1]);

    const [ch0color, setCh0Color] = useState([1.0, 0.0, 0.0]);
    const [ch1color, setCh1Color] = useState([0.0, 1.0, 0.0]);
    

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
            // Reference: https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1648
            setViewMatrix(mat4.clone(camera.view));
        }

        // Create a 2D camera for handling zoom and pan.
        const camera = createDom2dCamera(canvas,{
            isFixed: false,
            distance: 0.0,
            target: [0.0, 0.0],
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

        // Set the initial view matrix.
        camera.setView(viewMatrix);

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
                //zoom, // No longer used
                //targetX, // No longer used
                //targetY, // No longer used
                camera_view: viewMatrix,
                plot_id: 'my_plot',

                /*
                plot_type: 'Triangle',
                store_name: 'gaussian_quantiles_store',
                */

                /*
                plot_type: 'Scatterplot',
                store_name: 'gaussian_quantiles_store',
                plot_params: {
                    x_key: "/n_1000000/x_coords",
                    y_key: "/n_1000000/y_coords",
                    color_key: "/n_1000000/class_labels",
                    point_radius: pointRadius,
                }
                */
                
                
                /*
                plot_type: 'Scatterplot',
                store_name: 'mnist_store',
                plot_params: {
                    x_key: "/densmap/x_coords",
                    y_key: "/densmap/y_coords",
                    color_key: "/densmap/class_labels",
                    point_radius: pointRadius,
                }
                */
                
                
                plot_type: 'Bioimage',
                store_name: 'ome_ngff',
                plot_params: {
                    channel_indices: [0, 1],
                    channel_windows: [
                        ch0Window,
                        ch1Window,
                    ],
                    channel_colors: [
                        ch0color,
                        ch1color,
                    ],
                },
                
            };
            wasm.render_wasm(renderParams).then(arr => {
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
    }, [isWasmReady, viewMatrix, pointRadius, ch0Window, ch1Window, ch0color, ch1color]);

    return (
        <>
            <div style={{ width, height }}>
                <canvas
                    ref={canvasRef}
                    style={{ width, height, border: '1px solid black' }}
                    width={width}
                    height={height}
                />
            </div>
            <div>
                <label>Point Radius (for Scatterplot):</label>
                <input
                    type="range"
                    min={1.0}
                    max={100.0}
                    step={1.0}
                    value={pointRadius}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setPointRadius(newValue);
                    }}
                />
            </div>
            {/* Channel 0 controls: contrast min/max and r/g/b sliders */}
            <div>
                <label>Channel 0 Contrast Min:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch0Window[0]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh0Window([newValue, ch0Window[1]]);
                    }}
                />
                <label>Channel 0 Contrast Max:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch0Window[1]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh0Window([ch0Window[0], newValue]);
                    }}
                />
                <br/>
                <label>Channel 0 Red:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch0color[0]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh0Color([newValue, ch0color[1], ch0color[2]]);
                    }}
                />
                <label>Channel 0 Green:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch0color[1]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh0Color([ch0color[0], newValue, ch0color[2]]);
                    }}
                />
                <label>Channel 0 Blue:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch0color[2]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh0Color([ch0color[0], ch0color[1], newValue]);
                    }}
                />
            </div>
            {/* Channel 1 controls */}
            <div>
                <label>Channel 1 Contrast Min:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch1Window[0]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh1Window([newValue, ch1Window[1]]);
                    }}
                />
                <label>Channel 1 Contrast Max:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch1Window[1]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh1Window([ch1Window[0], newValue]);
                    }}
                />
                <br/>
                <label>Channel 1 Red:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch1color[0]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh1Color([newValue, ch1color[1], ch1color[2]]);
                    }}
                />
                <label>Channel 1 Green:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch1color[1]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh1Color([ch1color[0], newValue, ch1color[2]]);
                    }}
                />
                <label>Channel 1 Blue:</label>
                <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={ch1color[2]}
                    onChange={(e) => {
                        const newValue = parseFloat(e.target.value);
                        setCh1Color([ch1color[0], ch1color[1], newValue]);
                    }}
                />
            </div>
            
        </>
    );
}