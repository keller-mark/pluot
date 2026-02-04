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

const storeUrl = "https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/gaussian_quantiles.zarr";

export function CmvExample() {

    const store = useMemo(() => {
        return new FetchStore(storeUrl);
    }, [storeUrl]);

    const [pointRadius, setPointRadius] = useState(5.0);
    
    return (
        <div>
            <Pluot
                key="left"
                store={store}
                width={500}
                height={300}
                plotId={"left"}
                plotType={"LayeredPlot"}
                plotParams={{
                    layers: [
                        {
                            layer_type: "ZarrScatterplotLayer",
                            layer_params: {
                                layer_id: "layer_1",
                                data_unit_mode: "Data",
                                point_radius_unit_mode: "Pixels",
                                point_shape_mode: "Circle",
                                point_radius: pointRadius,
                                bounds: null,

                                x_key: "/n_1000000/x_coords",
                                y_key: "/n_1000000/y_coords",
                                color_key: "/n_1000000/class_labels",
                            }
                        }
                    ]
                }}
                mode={"2d"}
                marginLeft={0}
                marginTop={0}
                marginRight={0}
                marginBottom={0}

            />
            <Pluot
                key="right"
                store={store}
                width={500}
                height={300}
                plotId={"right"}
                plotType={"LayeredPlot"}
                plotParams={{
                    layers: [
                        {
                            layer_type: "ZarrScatterplotLayer",
                            layer_params: {
                                layer_id: "layer_2",
                                data_unit_mode: "Data",
                                point_radius_unit_mode: "Pixels",
                                point_shape_mode: "Circle",
                                point_radius: pointRadius,
                                bounds: null,

                                x_key: "/n_1000/x_coords",
                                y_key: "/n_1000/y_coords",
                                color_key: "/n_1000/class_labels",
                            }
                        }
                    ]
                }}
                mode={"2d"}
                marginLeft={0}
                marginTop={0}
                marginRight={0}
                marginBottom={0}
            />
            <div>
                <label>Point Radius:</label>
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
        </div>
    );
}