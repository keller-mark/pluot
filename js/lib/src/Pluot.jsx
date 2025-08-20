// TODO: once things are working with react,
// convert to use plain vanilla JS.
import React, { useLayoutEffect, useEffect, useRef, useState } from 'react';
import * as wasm from 'pluot';
import { FetchStore } from 'zarrita';
import { lru } from "./lru-store.js";

//const baseUrl = 'https://storage.googleapis.com/vitessce-demo-data/use-coordination/mnist.zarr';
const baseUrl = 'http://localhost:3005/data/out/mnist.zarr';

const stores = {
    // TODO: wrap store in a cache.
    // See https://github.com/hms-dbmi/vizarr/blob/862745c1c7c095748bbe97475da61807d5b49189/src/utils.ts#L47
    'my_store': lru(new FetchStore(baseUrl)),
}

// console.log(wasm);

// Define the global zarr_get function.
// TODO: figure out how to pass into wasm.default as a parameter, rather than setting on window/globally.
window.zarr_get = async (store_name, key) => {
    console.log(`zarr_get: store_name=${store_name}, key=${key}`);
    return stores[store_name].get(`/${key}`);
};

window.zarr_has = async (store_name, key) => {
    console.log(`zarr_has: store_name=${store_name}, key=${key}`);
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
            console.log('wasm.render')
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