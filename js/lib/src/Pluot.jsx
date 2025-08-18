import React, { useLayoutEffect, useEffect, useRef, useState } from 'react';
import * as wasm from 'pluot';
import * as zarr from 'zarrita';


// Define the global zarr_get function.
// TODO: figure out how to pass into wasm.default as a parameter, rather than setting on window/globally.
window.zarr_get = async (store_name, key) => {
    console.log(`zarr_get called with store_name: ${store_name}, key: ${key}`);

    // TODO: use zarrita here to create a zarr store and array.

    // Return fake data
    const n = 50000;
    const xs = new Int32Array(n);
    for (let i = 0; i < n; i++) {
    xs[i] = (Math.random()) * 1000.0;
    }
    return Promise.resolve(xs);
};

export function Pluot(props) {
    const {
        width,
        height,
        plotType = 'scatterplot',
        renderOnce = true,
    } = props;

    const canvasRef = useRef(null);
    const [isWasmReady, setIsWasmReady] = useState(false);

    useLayoutEffect(() => {

        const initWasm = async () => {
            await wasm.default();
            await wasm.set_panic_hook();
            setIsWasmReady(true);
        };
        initWasm();
        
    }, []);

    useEffect(() => {
        const canvas = canvasRef.current;
        if(!canvas || !isWasmReady) {
            return;
        }
        const ctx = canvas.getContext('2d');

        // Render once or every animation frame.
        // Define the function to render a single frame.
        function renderFrame() {
            wasm.render(width, height, plotType, 'my_store').then(arr => {
            const imageData = new ImageData(new Uint8ClampedArray(arr), width, height);
            ctx.putImageData(imageData, 0, 0);
            });
        }
        function animate() {
            renderFrame();
            requestAnimationFrame(animate);
        }

        // Initialize data and kick off the first render.
        if(renderOnce) {
            renderFrame();
        } else {
            requestAnimationFrame(animate);
        }
    }, [isWasmReady]);

    return (
        <div style={{ width, height }}>
            <canvas
                ref={canvasRef}
                style={{ width, height }}
                width={width}
                height={height}
            />
        </div>
    );
}