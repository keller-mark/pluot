import React, { useState, useMemo, lazy, Suspense } from 'react';
import { FetchStore } from 'zarrita';

const store = new FetchStore('https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/gaussian_quantiles.zarr');


const Pluot = lazy(async () => {
    // For 3d-view-controls.
    window.global = window;
    return {
        default: (await import('@pluot/react')).Pluot,
    };
});


export function Another(props) {
    
    return (
        <Suspense fallback={<p>Loading Pluot...</p>}>
            <Pluot
                width={500}
                height={500}
                plotId={"docs-example-scatterplot"}
                plotType={"LayeredPlot"}
                // TODO: host the store somewhere remotely.
                //storeName={"gaussian_quantiles_store"}
                store={store}
                plotParams={{
                    x_key: "/n_1000000/x_coords",
                    y_key: "/n_1000000/y_coords",
                    color_key: "/n_1000000/class_labels",
                    point_radius: 5.0,
                }}
                mode={"2d"}
                marginLeft={0}
                marginTop={0}
                marginRight={0}
                marginBottom={0}
            />
        </Suspense>
    );
}