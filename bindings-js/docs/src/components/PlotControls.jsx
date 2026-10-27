import React, { useMemo } from 'react';
import { useControls, Leva, button, folder } from 'leva';
import { createPlugin, useInputContext, Components } from 'leva/plugin';

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

export const QUANTITATIVE_COLORMAPS = [
  'Plasma',
  'Viridis',
  'Greys',
  'Magma',
  'Jet',
  'Bone',
  'Copper',
  'Density',
  'Inferno',
  'Cool',
  'Hot',
  'Spring',
  'Summer',
  'Autumn',
  'Winter',
  'Blues',
  'Greens',
  'Oranges',
  'Purples',
  'Reds',
];

export function useColormapOption(defaultColormap) {
  return useMemo(() => ({
    colormap: {
      value: defaultColormap ?? 'Viridis',
      options: QUANTITATIVE_COLORMAPS,
      label: 'Colormap',
    },
  }), [defaultColormap]);
}

function MultiSelect() {
  const { id, label, value, onUpdate, settings } = useInputContext();
  const onChange = (event) => {
    const selected = Array.from(event.target.selectedOptions, option => option.value);
    // Keep previously-selected values in their existing order (e.g., the gene order of heatmap columns),
    // rather than the order of the options list.
    onUpdate([
      ...value.filter(v => selected.includes(v)),
      ...selected.filter(v => !value.includes(v)),
    ]);
  };
  return (
    <Components.Row input>
      <Components.Label>{label}</Components.Label>
      <select
        id={id}
        multiple
        size={settings.size}
        value={value}
        onChange={onChange}
        style={{
          width: '100%',
          font: 'inherit',
          color: 'var(--leva-colors-highlight3)',
          backgroundColor: 'var(--leva-colors-elevation3)',
          border: 'none',
          borderRadius: 'var(--leva-radii-sm)',
        }}
      >
        {settings.options.map(option => (
          <option key={option} value={option}>{option}</option>
        ))}
      </select>
    </Components.Row>
  );
}

// The initial value is passed as `selected` rather than `value` because Leva
// only forwards `input.value` to normalize when a custom input has a `value` key, dropping the other settings.
export const multiSelect = createPlugin({
  normalize: ({ selected, options, size = 6 }) => ({ value: selected, settings: { options, size } }),
  component: MultiSelect,
});

export function resolvePlotParams(plotParams, controlValues) {
  return typeof plotParams === 'function' ? plotParams(controlValues) : plotParams;
}

export function usePlotControls(defaultOptions, plotSpecificOptions, callbacks) {
  const { onFullscreen, onFullwindow, onRenderToScript, enableVector = true } = callbacks ?? {};
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
    // Currently, need to conditionally show the Format selector, hiding when the plot renders > ~10,000 points.
    ...(enableVector ? ({
      format: {
        value: 'Raster',
        options: {
          Raster: 'Raster',
          Vector: 'Vector',
        },
        label: 'Graphics Format'
      },
    }) : {}),
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
    transparentBackground: {
      value: defaultOptions.transparentBackground ?? false,
      label: 'Transparent Background',
    },
    /*
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
    renderToScriptType: {
      value: defaultOptions.renderToScriptType ?? 'Json',
      options: {
        'JSON': 'Json',
        'Shell script': 'ScriptBash',
        'Rust script': 'ScriptRust',
        'Python script': 'ScriptPython',
        'R script': 'ScriptR',
        'HTML file (static)': 'ScriptHtml',
        'HTML file (React)': 'ScriptHtmlReact',
        'JSX script (React)': 'ScriptReact',
      },
      label: 'Render-to-Script Type'
    },
    ...(typeof onRenderToScript === 'function' ? ({
      'Render to Script': button(
        onRenderToScript,
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
