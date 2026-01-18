import React, { useState, useMemo, lazy, Suspense } from 'react';
import { FetchStore } from 'zarrita';


const Pluot = lazy(async () => {
    // For 3d-view-controls.
    window.global = window;
    return {
        default: (await import('@pluot/react')).Pluot,
    };
});

export function PluotWrapper(props) {
    const {
        storeUrl,
    } = props;

    const store = useMemo(() => {
        return new FetchStore(storeUrl);
    }, [storeUrl]);
    
    return (
        <Suspense fallback={<p>Loading Pluot...</p>}>
            <Pluot
                store={store}
                width={500}
                height={500}
                plotId={"example-plot"}
                plotType={"LayeredPlot"}
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
                {...props}
            />
        </Suspense>
    );
}