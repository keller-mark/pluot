import React, { useState, useMemo, lazy, Suspense } from 'react';
import { FetchStore } from 'zarrita';
import { Pluot } from '@pluot/react';
import { PlotControls, usePlotControls } from './PlotControls.jsx';

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
    plotId = "example-plot",
  } = props;

  const controlValues = usePlotControls();
  console.log(controlValues);

  const store = useMemo(() => {
    return new FetchStore(storeUrl);
  }, [storeUrl]);

  const width = controlValues.size.width;
  const height = controlValues.size.height;

  return (
    <>
      <Pluot
        store={store}
        width={width}
        height={height}
        plotId={plotId}
        plotType={"LayeredPlot"}
        plotParams={{
            layers: []
        }}
        mode={"2d"}
        marginLeft={0}
        marginTop={0}
        marginRight={0}
        marginBottom={0}
        {...props}
      />
      <PlotControls />
    </>
  );
}
