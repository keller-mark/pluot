import React from 'react';
import { useControls, Leva, button, folder } from 'leva';

const titleBar = {
  title: "Plot Controls",
  drag: false,
  filter: false,
  position: "relative",
};

// Reference: https://leva.pmnd.rs/?path=/story/advanced-theme--default
const theme = {
  colors: {
    "elevation1": "#5c5c5c", // BG of top part.
    "highlight1": "#ffffff", // FG of top part.
    "highlight2": "#ffffff", // Label text.
    "accent2": "#e7711f", // Colors of buttons, checkboxes, sliders, etc.
  },
  fontSizes: {
    root: "12px"
  },
  sizes: {
    titleBarHeight: "30px"
  }
};

export function usePlotControls(defaultOptions, plotSpecificOptions, callbacks) {
  const { onFullscreen, onFullwindow } = callbacks ?? {};
  // TODO: If defaultOptions are provided, use them to populate the default values here.
  // plotSpecificOptions will be an object like
  /*
      {
        pointRadius: {
          value: 5,
          min: 0,
          max: 100,
          label: 'Point Radius'
        }
      }
  */
  return useControls({
    // TODO: split "interactive" into multiple aspects:
    // Camera Enabled, Hover-based picking, Click-based picking, etc.
    // Perhaps use a conditional Leva folder to do this,
    // where the checked "Interactive" checkbox enables the folder to be shown?
    /*
    interactive: {
      value: true,
      label: 'Interactive',
    },
    */
    size: {
      value: {
        width: defaultOptions.width ?? 500,
        height: defaultOptions.height ?? 500
      },
      min: 0,
      step: 1,
      joystick: false,
      lock: true,
      label: 'Plot Size'
    },
    verticalMargins: {
      value: {
        bottom: defaultOptions.marginBottom ?? 0,
        top: defaultOptions.marginTop ?? 0,
      },
      min: 0,
      step: 1,
      joystick: false,
      lock: true,
      label: 'Margins (Vertical)'
    },
    horizontalMargins: {
      value: {
        left: defaultOptions.marginLeft ?? 0,
        right: defaultOptions.marginRight ?? 0,
      },
      min: 0,
      step: 1,
      joystick: false,
      lock: true,
      label: 'Margins (Horizontal)'
    },
    aspectRatioMode: {
      value: defaultOptions.aspectRatioMode ?? 'Contain',
      options: {
        'Contain (fit)': 'Contain',
        'Cover (fill)': 'Cover',
        'Ignore (stretch)': 'Ignore',
      },
      label: 'Aspect Ratio Mode'
    },
    aspectRatioAlignmentMode: {
      value: defaultOptions.aspectRatioAlignmentMode ?? 'Center',
      options: {
        'Center': 'Center',
        'Start': 'Start',
        'End': 'End',
      },
      label: 'Aspect Ratio Alignment Mode'
    },
    debugMargins: {
      value: false,
      label: 'Show Margins',
      hint: 'For debugging, display a 1px border indicating margin boundaries.'
    },
    // TODO: need to conditionally show the Format selector, hiding when the plot renders > ~10,000 points.
    /*
    format: {
      value: 'Raster',
      options: {
        Raster: 'Raster',
        Vector: 'Vector',
      },
      label: 'Graphics Format'
    },
    'Reset Camera': button(
      get => alert(`Interactive value is ${get('interactive')}`),
      { disabled: false }
    ),
    // TODO: download button
    */
    ...(typeof onFullwindow === 'function' ? ({
      'Full Window': button(
        onFullwindow,
        { disabled: false }
      ),
    }) : {}),
    ...(typeof onFullscreen === 'function' ? ({
      'Full Screen': button(
        onFullscreen,
        { disabled: false }
      ),
    }) : {}),
    ...(plotSpecificOptions ? ({
      'Plot-Specific Options': folder(
        plotSpecificOptions,
        { collapsed: false }
      ),
    }) : {}),
  }, [plotSpecificOptions]);
}

export function PlotControls(props) {
  const {
    showControls = true,
    float = false,
  } = props;
  return (
    <div className="plot-controls-container" style={{ ...(float ? {} : { margin: '10px 0' }) }}>
      <style>{`
        .plot-controls-container {
          /* We need to override this Starlight CSS property to prevent it from applying margins within the Leva children divs */
          --sl-content-gap-y: 0;
        }
      `}</style>
      <Leva
        collapsed={true}
        fill={!float}
        titleBar={titleBar}
        hideCopyButton={true}
        theme={theme}
        hidden={!showControls}
      />
    </div>
  );
}
