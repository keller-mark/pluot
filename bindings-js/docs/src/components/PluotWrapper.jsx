import React, { useMemo, useRef, useCallback, useEffect, useState } from 'react';
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

    // Option to provide "plot-specific" options objects for plot types that need them
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

  const [fsWidth, setFsWidth] = useState(null);
  const [fsHeight, setFsHeight] = useState(null);

  const divRef = useRef(null);

  // Handling of fullscreenchange event.
  useEffect(() => {
    let discarded = false;
    const onFSChange = (event) => {
      console.log(event);

      if (document.fullscreenElement) {
        // Entering
        setTimeout(() => {
          // We need to delay this a tiny bit. In Chrome, getBoundingClientRect initially
          // returns a value that seems to correspond to the size of the Chrome window,
          // rather than the full screen size.
          const fullscreenSize = document.fullscreenElement.getBoundingClientRect();

          if(!discarded) {
            setFsHeight(fullscreenSize.height);
            setFsWidth(fullscreenSize.width);
          }
        }, 100);
      } else {
        // Exiting
        setFsHeight(null);
        setFsWidth(null);
      }
    };
    document.addEventListener("fullscreenchange", onFSChange);

    return () => {
      discarded = true;
      document.removeEventListener("fullscreenchange", onFSChange);
    };
  }, []);

  const onFullscreen = useCallback(() => {
    const divEl = divRef.current;

    if (!document.fullscreenElement) {
        // If the document is not in full screen mode
        // make the video full screen
        divEl.requestFullscreen();
      } else {
        // Otherwise exit the full screen mode.
        document.exitFullscreen?.();
      }
  }, []);

  const controlValues = usePlotControls(defaultOptions, plotSpecificOptions, { onFullscreen });
  console.log(controlValues);

  const derivedPlotParams = useMemo(() => {
    if (!plotParams) {
      throw new Error("PlotWrapper could not find plotParams.");
    }
    if (typeof plotParams === 'function') {
      return plotParams(controlValues);
    }
    return plotParams;
  }, [plotParams, controlValues]);

  const { aspectRatioMode, aspectRatioAlignmentMode, format, debugMargins } = controlValues;
  const width = controlValues.size.width;
  const height = controlValues.size.height;
  const marginLeft = controlValues.horizontalMargins.left;
  const marginRight = controlValues.horizontalMargins.right;
  const marginTop = controlValues.verticalMargins.top;
  const marginBottom = controlValues.verticalMargins.bottom;

  // TODO: render the PlotControls over the plot in Fullscreen mode
  // TODO: render the Loading indicator over the plot in Fullscreen mode

  return (
    <>
      <div ref={divRef}>
        <Pluot
          store={storeUrl}
          width={fsWidth ?? width}
          height={fsHeight ?? height}
          plotId={plotId}
          plotType={plotType}
          plotParams={derivedPlotParams}
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
      </div>
      <PlotControls
        showControls={showControls}
      />

    </>
  );
}
