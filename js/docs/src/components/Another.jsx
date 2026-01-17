import React, { useState, useMemo, lazy, Suspense } from 'react';


const Pluot = lazy(async () => {
    // For 3d-view-controls.
    window.global = window;
    return {
        default: (await import('pluot-wrapper')).Pluot,
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
                storeName={"gaussian_quantiles_store"}
                plotParams={{
                    x_key: "/n_1000000/x_coords",
                    y_key: "/n_1000000/y_coords",
                    color_key: "/n_1000000/class_labels",
                    point_radius: 5.0,
                }}
                mode={"2d"}
            />
        </Suspense>
    );
}