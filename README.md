# pluot

Implement once, pluot everywhere.
Create declarative static and interactive plots using WGPU and Rust/WASM.

## Principles

The frontend should never "touch" the data.
The frontend may, however, register data-loading functions that are called by the Rust code.


The frontend will specify visual properties and data-related properties and expressions, for example colormaps, viewState (zoom/pan), data filtering expressions via parameters.
The data filtering operations themselves will always be performed in Rust.
For example, given a viewState, the Rust code may load certain chunks of data. Given a data filtering expression, it may then filter that data. Finally, it will perform the render pass and return the arraybuffer (which the JS code will render to a Canvas).


The Rust code should only be concerned with rendering a single plot, and should not care whether the caller is intending to use the result in a static or interactive context.

Interactions such as picking can be performed by the JS code requesting the data point that is closest to a given pixel coordinate.

## JS API

```js

import { init, render } from 'pluot';

// How should this work in other languages?
// What should this return? Arrow vector? TypedArray? Arrow table IPC?
// Should this be more aware of tiling/multi-resolution data? How would it handle XYZCT imaging or volumetric or mesh data?

async function dataGetter(dataKey, columnExpression, rowExpression) {
    // Given the key, return something like DeckGL's binary format.
    // If the underlying data format/provider supports it, we may only want to load a subset of rows or columns.
    // For instance, if the data format is spatially-indexed, we may be able to load a subset of rows based on the rowExpression (e.g., derived from viewState and width/height).
    return {
        src: {
            columnA: new Uint8Array([]),
            columnB: new Uint8Array([]),
            columnC: new Uint8Array([])
        },
        length: 10
    };
}

await init(dataGetter);

const arr = await render({
    width: 500,
    height: 500,
    /*
    viewState: {
        // Frontend should manage this state
        zoom: 0,
        target: [2, 2],
    },
    // TODO: how to specify 2D vs. 3D?
    coordinateSystem: 'CARTESIAN', // also support 'GENOMIC'
    // Option 1: DeckGL-like API
    // This delegates more flexibility to the client / caller.
    layers: [
        new ScatterplotLayer({
            dataKey: 'my_dataset_key',
        }),
        new PolygonLayer({
            
        })
    ],
    */
    // Option 2: Vitessce-like view-based API
    // The rust code will know how to render a scatterplot.
    // This delegates more responsibility to the rust code, and limits the flexibility.
    // However that is OK because the intention is that the rust code should be where the plotting code lives.
    // The Rust code can have its own internal deckGL-like APIs to render a scatterplot.
    viewType: 'scatterplot',
    // Pass any coordination values that the Rust code knows about.
    // The rust code will use these 
    coordinationValues: {
        dataset: 'my_dataset_key',
        embeddingType: 'UMAP',
        pointLayer: [
            {
                obsType: 'cell',
                obsSetFilter: [['cell_type', 'immune']],
                obsSetSelection: [['cell_type', 'immune', 'B cell'], ['cell_type', 'immune', 'T cell']],
                obsSetColor: [
                    { path: ['cell_type', 'immune', 'B cell'], color: [255, 0, 0] }
                ],
                embeddingZoom: 0,
                embeddingTargetX: 0,
                embeddingTargetY: 0,
                embeddingTargetZ: null,
            },
        ],
        contourLayer: {
            
        }
    },
    // Another, more complex view type:
    viewType: 'spatial',
    coordinationValues: {
        dataset: 'my_dataset_key',
        // May have nested coordination values to support layer->channel pattern.
        imageLayer: [
            {
                imageChannel: [
                    {

                    }
                ]
            }
        ],
        segmentationLayer: [
            {
                segmentationChannel: [
                    {
                        'test'
                    }
                ]
            }
        ]
    },
    // Simple statistical view type:
    viewType: 'featureValueDistributionHistogram',
    coordinationValues: {
        dataset: 'my_dataset_key',
        obsType: 'cell',
        featureType: 'gene',
        featureSelection: 'CD4',

    },
    // Genomic view type:
    viewType: 'genomicProfiles',
    coordinationValues: {
        dataset: 'my_dataset_key'
    }
})
```