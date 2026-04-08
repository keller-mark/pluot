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
    "accent2": "#6b47cb", // Colors of buttons, checkboxes, sliders, etc.
  },
  fontSizes: {
    root: "12px"
  },
  sizes: {
    titleBarHeight: "30px"
  }
};

export function usePlotControls() {
  return useControls({
    interactive: {
      value: true,
      label: 'Interactive',
    },
    size: {
      value: {
        width: 200,
        height: 300
      },
      joystick: false,
      lock: true,
      label: 'Plot Size'
    },
    format: {
      value: 'raster',
      options: {
        Raster: 'raster',
        Vector: 'vector',
      },
      label: 'Graphics Format'
    },
    verticalMargins: {
      value: {
        bottom: 0,
        top: 0,
      },
      joystick: false,
      lock: true,
      label: 'Margins (Vertical)'
    },
    horizontalMargins: {
      value: {
        left: 0,
        right: 0,
      },
      joystick: false,
      lock: true,
      label: 'Margins (Horizontal)'
    },
    aspectRatioMode: {
      value: 'contain',
      options: {
        'Contain (fit)': 'contain',
        'Cover (fill)': 'cover',
        'Ignore (stretch)': 'ignore',
      },
      label: 'Aspect Ratio Mode'
    },
    'Reset Camera': button(
      get => alert(`Interactive value is ${get('interactive')}`),
      { disabled: false }
    ),
    'Full Screen': button(
      get => alert(`Interactive value is ${get('interactive')}`),
      { disabled: false }
    ),
    'Plot-Specific Options': folder({
      pointRadius: {
        value: 5,
        min: 0,
        max: 100,
        label: 'Point Radius'
      }
    }, { collapsed: false })
  });
}

export function PlotControls() {
  return (
    <div className="plot-controls-container" style={{ margin: '10px 0' }}>
      <style>{`
        .plot-controls-container {
          /* We need to override this Starlight CSS property to prevent it from applying margins within the Leva children divs */
          --sl-content-gap-y: 0;
        }
      `}</style>
      <Leva
        fill={true}
        titleBar={titleBar}
        hideCopyButton={true}
        theme={theme}
      />
    </div>
  );
}
