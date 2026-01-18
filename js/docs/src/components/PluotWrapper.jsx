import React, { useState, useMemo, lazy, Suspense } from 'react';
import { FetchStore } from 'zarrita';
import { Pluot } from '@pluot/react';

/*
// We need to use a dynamic import here, because Pluot accesses `window`
// at the top-level, which causes issues during server-side rendering.
// Even though we pass `client:only` to the PluotWrapper component in Astro,
// Astro still tries to import from its JS file during the build step,
// which fails.
const Pluot = lazy(async () => {
    return {
        default: (await import('@pluot/react')).Pluot,
    };
});
*/

export function PluotWrapper(props) {
    const {
        storeUrl,
    } = props;

    const store = useMemo(() => {
        return new FetchStore(storeUrl);
    }, [storeUrl]);
    
    return (
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
    );
}