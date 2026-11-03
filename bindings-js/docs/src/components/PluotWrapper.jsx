import React, { useMemo, useRef, useCallback, useEffect, useEffectEvent, useState } from 'react';
import { createPortal } from 'react-dom';
import { Pluot, render_to_script_wasm, renderToString, renderToImageData, normalizeStores } from '@pluot/react';
import { PlotControls, usePlotControls } from './PlotControls.jsx';

const SCRIPT_EXTENSIONS = {
  Json: 'json',
  ScriptBash: 'sh',
  ScriptRust: 'rs',
  ScriptPython: 'py',
  ScriptIpynb: 'ipynb',
  ScriptMoPy: 'mo.py',
  ScriptR: 'R',
  ScriptHtml: 'html',
  ScriptHtmlReact: 'html',
  ScriptReact: 'jsx',
};

const IDENTITY_CAMERA = new Float32Array([
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1, 0,
  0, 0, 0, 1,
]);

const NOOP = () => { };

function defaultOnClick(obj) {
  window.alert(JSON.stringify(obj, null, 2));
}

function downloadBlob(blob, filename) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function imageDataToPngBlob(imageData) {
  const canvas = document.createElement('canvas');
  canvas.width = imageData.width;
  canvas.height = imageData.height;
  canvas.getContext('2d').putImageData(imageData, 0, 0);
  return new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
}

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
    // 'Raster' | 'Vector'
    format: defaultFormat = 'Raster',

    // Option to provide "plot-specific" options objects for plot types that need them
    // (e.g., with pointSize option for scatterplots, channel controls for bioimaging, etc.)
    plotSpecificOptions = null,
    cameraMatrix: cameraMatrixProp = null,
    cameraFilter = null,
    limitExtent = false,
    enableClick = true,
    enableTooltip = false,
    enableCamera = true,
    // TODO: remove this option upon completion of https://github.com/keller-mark/pluot/issues/221
    enableVector = true,

    onHover = null, // Pass through to Pluot component
    // Either a string or a function of the control values.
    extentKey = null,

    // Brushing props, passed through to the Pluot component.
    brushMode,
    brushUnitsModeX,
    brushUnitsModeY,
    enableBrushCreate = false,
    enableBrushEdit = false,
    enableBrushClear = false,
    brush,
    onBrush,
    onBrushEnd,
    onBrushClear,
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
    format: defaultFormat,
  };

  // 'none' | 'fullwindow' | 'fullscreen'
  const [mode, setMode] = useState('none');
  const [containerSize, setContainerSize] = useState(null);

  const [cameraMatrix, setCameraMatrix] = useState(cameraMatrixProp);

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
    }
  });

  useEffect(() => {
    document.addEventListener("fullscreenchange", onFSChange);

    return () => {
      document.removeEventListener("fullscreenchange", onFSChange);
    };
  }, []);

  // In full-window/full-screen mode the plot fills this div; otherwise its width
  // caps the plot width so the plot fits narrow screens. The div is remounted
  // (via the portal) when the mode changes, so re-observe on each change.
  useEffect(() => {
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
      setContainerSize({ width, height });
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

  const controlValuesRef = useRef(null);
  const derivedPlotParamsRef = useRef(null);
  const [scriptResult, setScriptResult] = useState(null);
  const getStaticRenderParams = () => ({
    format: controlValuesRef.current.format ?? defaultFormat,

    stores: normalizeStores({ store: storeUrl, plotId, register: false }),
    plot_params: derivedPlotParamsRef.current,
    camera_view: cameraMatrix,

    width: controlValuesRef.current.size.width,
    height: controlValuesRef.current.size.height,
    margin_left: controlValuesRef.current.horizontalMargins.left,
    margin_right: controlValuesRef.current.horizontalMargins.right,
    margin_top: controlValuesRef.current.verticalMargins.top,
    margin_bottom: controlValuesRef.current.verticalMargins.bottom,
    aspect_ratio_mode: controlValuesRef.current.aspectRatioMode,
    aspect_ratio_alignment_mode: controlValuesRef.current.aspectRatioAlignmentMode,

    device_pixel_ratio: window.devicePixelRatio,
    view_mode: viewMode,
    pickable: false,
    plot_id: plotId,
    plot_type: plotType,
    // Note: the below settings are optimized for static plotting.
    timeout: null, // Note: will not have any effect when wait_for_store_gets is false.
    wait_for_store_gets: true,
    cache_enabled: true,
    svg_compression_enabled: false,
    svg_include_document: true,
  });

  const onRenderToScript = useEffectEvent(async () => {
    const codeFormat = controlValuesRef.current.renderToScriptType;

    // Wrap render_wasm in try/catch, to handle Rust panics.
    let codeStringResult;
    try {
      codeStringResult = await render_to_script_wasm(getStaticRenderParams(), codeFormat);

    } catch (error) {
      console.error("Error during wasm.render_wasm (rendering to script):", error);
      return;
    }

    setScriptResult(codeStringResult);
  });

  const scriptFormat = controlValuesRef.current?.renderToScriptType;

  const onDownloadScript = useEffectEvent(() => {
    if (scriptResult === null || scriptFormat === null) {
      return;
    }

    const ext = SCRIPT_EXTENSIONS[scriptFormat] ?? 'txt';
    downloadBlob(new Blob([scriptResult], { type: 'text/plain' }), `${plotId}.${ext}`);
  });

  const onDownloadPlot = useEffectEvent(async () => {
    const renderParams = { ...getStaticRenderParams(), plotId, register: false };

    // Wrap rendering in try/catch, to handle Rust panics.
    try {
      if (renderParams.format === 'Vector') {
        const { plot: svgString } = await renderToString(renderParams);
        downloadBlob(new Blob([svgString], { type: 'image/svg+xml' }), `${plotId}.svg`);
      } else {
        const { plot: imageData } = await renderToImageData(renderParams);
        downloadBlob(await imageDataToPngBlob(imageData), `${plotId}.png`);
      }
    } catch (error) {
      console.error("Error during rendering for download:", error);
    }
  });

  const controlValues = usePlotControls(defaultOptions, plotSpecificOptions, { onFullscreen, onFullwindow, onDownloadPlot, onRenderToScript, enableVector });
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

  const derivedExtentKey = typeof extentKey === 'function' ? extentKey(controlValues) : extentKey;

  // A user-controlled camera matrix bypasses the extent query, so it must be cleared for the new extent to take effect.
  const [prevExtentKey, setPrevExtentKey] = useState(derivedExtentKey);
  if (derivedExtentKey !== prevExtentKey) {
    setPrevExtentKey(derivedExtentKey);
    setCameraMatrix(null);
  }

  const { aspectRatioMode, aspectRatioAlignmentMode, debugMargins } = controlValues;
  // The format control is absent when enableVector is false.
  const format = controlValues.format ?? defaultFormat;
  const width = controlValues.size.width;
  const height = controlValues.size.height;
  const marginLeft = controlValues.horizontalMargins.left;
  const marginRight = controlValues.horizontalMargins.right;
  const marginTop = controlValues.verticalMargins.top;
  const marginBottom = controlValues.verticalMargins.bottom;

  let plotWidth = width;
  let plotHeight = height;
  if (isFullscreenOrWindow && containerSize) {
    plotWidth = containerSize.width;
    plotHeight = containerSize.height;
  } else if (containerSize && containerSize.width > 0 && containerSize.width < width) {
    plotWidth = Math.floor(containerSize.width);
    plotHeight = Math.floor(height * plotWidth / width);
  }

  controlValuesRef.current = { ...controlValues };
  derivedPlotParamsRef.current = { ...derivedPlotParams };

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
          width={plotWidth}
          height={plotHeight}
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
          cameraMatrix={enableCamera && cameraMatrix ? cameraMatrix : null}
          setCameraMatrix={setCameraMatrix}
          cameraFilter={cameraFilter}
          limitExtent={limitExtent}
          extentKey={derivedExtentKey}
          enableClick={enableClick}
          enableTooltip={enableTooltip}
          backgroundColor={"#fff"}
          onHover={onHover}
          onClick={defaultOnClick}
          brushMode={brushMode}
          brushUnitsModeX={brushUnitsModeX}
          brushUnitsModeY={brushUnitsModeY}
          enableBrushCreate={enableBrushCreate}
          enableBrushEdit={enableBrushEdit}
          enableBrushClear={enableBrushClear}
          brush={brush}
          onBrush={onBrush}
          onBrushEnd={onBrushEnd}
          onBrushClear={onBrushClear}
        />
      </div>
      <PlotControls
        showControls={showControls}
        float={isFullscreenOrWindow}
      />
      {scriptResult !== null ? (
        <>
          <button onClick={onDownloadScript}>
            Download {plotId}.{SCRIPT_EXTENSIONS[scriptFormat] ?? 'txt'}
          </button>
          <pre>
            {scriptResult}
          </pre>
        </>
      ) : null}
    </div>
  );

  return isFullscreenOrWindow ? createPortal(content, document.body) : content;
}
