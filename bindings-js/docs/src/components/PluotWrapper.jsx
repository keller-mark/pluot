import React, { useMemo } from 'react';
import { FetchStore } from 'zarrita';
import { Pluot } from '@pluot/react';
import { PlotControls, usePlotControls } from './PlotControls.jsx';


export function PluotWrapper(props) {
  const {
    showControls = true,

    plotId = "example-plot",
    plotType = "LayeredPlot",
    storeUrl,
    plotParams,
    viewMode = "2d",

    // If defaults for margins, sizes, etc. are provided here,
    // pass to usePlotControls so that the controls can use them for the initial values.
    width: defaultWidth,
    height: defaultHeight,
    marginLeft: defaultMarginLeft,
    marginRight: defaultMarginRight,
    marginTop: defaultMarginTop,
    marginBottom: defaultMarginBottom,
    aspectRatioMode: defaultAspectRatioMode,
    aspectRatioAlignmentMode: defaultAspectRatioAlignmentMode,

    // TODO: define "plot-specific" options objects for plot types that need them
    // (e.g., with pointSize option for scatterplots, channel controls for bioimaging, etc.)
    plotSpecificOptions = null,
  } = props;

  const defaultOptions = {
    width: defaultWidth,
    height: defaultHeight,
    marginLeft: defaultMarginLeft,
    marginRight: defaultMarginRight,
    marginTop: defaultMarginTop,
    marginBottom: defaultMarginBottom,
    aspectRatioMode: defaultAspectRatioMode,
    aspectRatioAlignmentMode: defaultAspectRatioAlignmentMode,
  };

  const controlValues = usePlotControls(defaultOptions, plotSpecificOptions);
  console.log(controlValues);

  const store = useMemo(() => {
    return new FetchStore(storeUrl);
  }, [storeUrl]);

  const { aspectRatioMode, aspectRatioAlignmentMode, format, debugMargins } = controlValues;
  const width = controlValues.size.width;
  const height = controlValues.size.height;
  const marginLeft = controlValues.horizontalMargins.left;
  const marginRight = controlValues.horizontalMargins.right;
  const marginTop = controlValues.verticalMargins.top;
  const marginBottom = controlValues.verticalMargins.bottom;

  return (
    <>
      <Pluot
        store={store}
        width={width}
        height={height}
        plotId={plotId}
        plotType={plotType}
        plotParams={plotParams ?? ({
          layers: []
        })}
        viewMode={viewMode}
        marginLeft={marginLeft}
        marginTop={marginTop}
        marginRight={marginRight}
        marginBottom={marginBottom}
        aspectRatioMode={aspectRatioMode}
        aspectRatioAlignmentMode={aspectRatioAlignmentMode}
        format={format}
        debugMargins={debugMargins}
      />
      <PlotControls
        showControls={showControls}
      />
    </>
  );
}
