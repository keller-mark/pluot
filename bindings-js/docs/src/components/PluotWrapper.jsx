import React, { useMemo, useRef, useCallback, useEffect, useEffectEvent, useState } from 'react';
import { createPortal } from 'react-dom';
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
    cameraMatrix = null,
    enablePicking = true,
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

  // 'none' | 'fullwindow' | 'fullscreen'
  const [mode, setMode] = useState('none');
  const [fsWidth, setFsWidth] = useState(null);
  const [fsHeight, setFsHeight] = useState(null);

  const divRef = useRef(null);

  const isFullwindow = mode === 'fullwindow';
  const isFullscreen = mode === 'fullscreen';
  const isFullscreenOrWindow = mode !== 'none';

  // Handling of fullscreenchange event. This fires both when we request
  // fullscreen ourselves, and when the browser exits it natively (Escape,
  // browser/OS chrome, etc.). Registered once; reads the latest `mode` via
  // useEffectEvent instead of a stale closure.
  const onFSChange = useEffectEvent(() => {
    if (document.fullscreenElement) {
      setMode('fullscreen');
    } else if (mode !== 'fullwindow') {
      // A real exit back to neither mode. If mode is already 'fullwindow',
      // this exit is instead part of a controlled fullscreen -> full-window
      // transition (see onFullwindow below), so leave it alone.
      setMode('none');
      setFsWidth(null);
      setFsHeight(null);
    }
  });

  useEffect(() => {
    document.addEventListener("fullscreenchange", onFSChange);

    return () => {
      document.removeEventListener("fullscreenchange", onFSChange);
    };
  }, []);

  // While in full-window or full-screen mode, the parent div is resized
  // (either by our own fixed-position styles, or by the browser's native
  // fullscreen layout), so observe it and keep fsWidth/fsHeight in sync.
  useEffect(() => {
    if (!isFullscreenOrWindow) {
      return undefined;
    }
    const divEl = divRef.current;
    if (!divEl) {
      return undefined;
    }

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }
      const { width, height } = entry.contentRect;
      setFsWidth(width);
      setFsHeight(height);
    });
    observer.observe(divEl);

    return () => {
      observer.disconnect();
    };
  }, [isFullscreenOrWindow]);

  const onFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      // Enter full-screen mode, whether we were previously in full-window
      // mode or in neither mode.
      document.body?.requestFullscreen();
    } else {
      // Otherwise exit full-screen mode.
      document.exitFullscreen?.();
    }
  }, []);

  const onFullwindow = useEffectEvent(() => {
    if (mode !== 'fullwindow') {
      // Set mode first: if we're currently in native full-screen, exiting it
      // below fires fullscreenchange, whose handler checks this ref/state and
      // leaves 'fullwindow' alone instead of resetting to 'none'.
      setMode('fullwindow');
      if (document.fullscreenElement) {
        document.exitFullscreen?.();
      }
    } else {
      // Otherwise exit.
      setMode('none');
      setFsWidth(null);
      setFsHeight(null);
      if (document.fullscreenElement) {
        document.exitFullscreen?.();
      }
    }
  }, []);

  useEffect(() => {
    // If in full window mode, listen for escape keypresses to exit this mode.

    if (isFullwindow) {
      function onKeypress(event) {
        const isEscape = ["Escape", "Esc"].includes(event.key) || event.keyCode === 27;
        if (isEscape) {
          setMode('none');
          setFsHeight(null);
          setFsWidth(null);
          if (document.fullscreenElement) {
            // Guard against the (transient) case where full-window mode was
            // entered directly from full-screen mode and the native exit is
            // still in flight; make sure we always fully exit too.
            document.exitFullscreen?.();
          }
        }
      }

      document.addEventListener("keydown", onKeypress);

      return () => {
        document.removeEventListener("keydown", onKeypress);
      }
    }

    return () => { };
  }, [isFullwindow]);

  const controlValues = usePlotControls(defaultOptions, plotSpecificOptions, { onFullscreen, onFullwindow });
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

  console.log(cameraMatrix)

  const content = (
    <div ref={divRef} style={(isFullscreenOrWindow ? ({
      position: 'fixed',
      zIndex: 11,
      top: 0,
      bottom: 0,
      left: 0,
      right: 0,
      width: '100wh',
      height: '100vh',
      overflow: 'hidden',
    }) : {})}>
      <div>
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
          cameraMatrix={cameraMatrix}
          enablePicking={enablePicking}
          backgroundColor={"#fff"}
        />
      </div>
      <PlotControls
        showControls={showControls}
        float={isFullscreenOrWindow}
      />

    </div>
  );

  return isFullscreenOrWindow ? createPortal(content, document.body) : content;
}
