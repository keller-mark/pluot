import React, { useState } from "react";
import { Pluot } from "pluot-wrapper";

const DEMOS = {
  triangle: {
    plot_type: "Triangle",
    store_name: "",
    plot_params: {},
  },
  scatterplot_1m: {
    plot_type: "Scatterplot",
    store_name: "gaussian_quantiles_store",
    plot_params: {
      x_key: "/n_1000000/x_coords",
      y_key: "/n_1000000/y_coords",
      color_key: "/n_1000000/class_labels",
      //point_radius: pointRadius,
    },
  },
  scatterplot_3d: {
    plot_type: "Scatterplot3d",
    store_name: "gaussian_quantiles_store",
    plot_params: {
      x_key: "/n_100000/x_coords",
      y_key: "/n_100000/y_coords",
      z_key: "/n_100000/z_coords",
      color_key: "/n_100000/class_labels",
      point_radius: 5.0,
    },
  },
  scatterplot_mnist: {
    plot_type: "Scatterplot",
    store_name: "mnist_store",
    plot_params: {
      x_key: "/densmap/x_coords",
      y_key: "/densmap/y_coords",
      color_key: "/densmap/class_labels",
      //point_radius: pointRadius,
    },
  },
  ome_ngff: {
    plot_type: "Bioimage",
    store_name: "ome_ngff",
    plot_params: {
      // target_z: zIndex,
      // channel_indices: [0, 1],
      // channel_windows: [
      //     ch0Window,
      //     ch1Window,
      // ],
      // channel_colors: [
      //     ch0color,
      //     ch1color,
      // ],
    },
  },
  bar_plot: {
    // Reference: https://altair-viz.github.io/gallery/bar_chart_with_highlighted_bar.html
    plot_type: "BarPlot",
    store_name: "wheat",
    plot_params: {
      x_key: "/year",
      y_key: "/wheat",
    }
  },
};

export function Demo() {
  const [currPlotId, setCurrPlotId] = useState("scatterplot_mnist");

  const plotType = DEMOS[currPlotId].plot_type;
  const plotParams = DEMOS[currPlotId].plot_params;
  const storeName = DEMOS[currPlotId].store_name;

  const [pointRadius, setPointRadius] = useState(5.0);

  const [ch0Window, setCh0Window] = useState([0.0, 0.1]);
  const [ch1Window, setCh1Window] = useState([0.0, 0.1]);
  const [ch0color, setCh0Color] = useState([1.0, 0.0, 0.0]);
  const [ch1color, setCh1Color] = useState([0.0, 1.0, 0.0]);
  const [zIndex, setZIndex] = useState(99);

  return (
    <div>
      <select
        value={currPlotId}
        onChange={(e) => setCurrPlotId(e.target.value)}
      >
        {Object.keys(DEMOS).map((plotId) => (
          <option key={plotId}>{plotId}</option>
        ))}
      </select>
      <Pluot
        width={800}
        height={800}
        plotId={currPlotId}
        plotType={plotType}
        storeName={storeName}
        plotParams={
          plotType === "Triangle"
            ? undefined
            : {
                ...plotParams,
                ...(plotType === "Scatterplot"
                  ? {
                      point_radius: pointRadius,
                    }
                  : {}),
                ...(plotType === "Bioimage"
                  ? {
                      target_z: zIndex,
                      channel_indices: [0, 1],
                      channel_windows: [ch0Window, ch1Window],
                      channel_colors: [ch0color, ch1color],
                    }
                  : {}),
              }
        }
        mode={plotType === "Scatterplot3d" ? "3d" : "2d"}
      />
      {plotType === "Scatterplot" ? (
        <div>
          <label>Point Radius (for Scatterplot):</label>
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
      ) : null}
      {plotType === "Bioimage" ? (
        <>
          <div>
            {/* Channel 0 controls: contrast min/max and r/g/b sliders */}
            <label>Channel 0 Contrast Min:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch0Window[0]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh0Window([newValue, ch0Window[1]]);
              }}
            />
            <label>Channel 0 Contrast Max:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch0Window[1]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh0Window([ch0Window[0], newValue]);
              }}
            />
            <br />
            <label>Channel 0 Red:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch0color[0]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh0Color([newValue, ch0color[1], ch0color[2]]);
              }}
            />
            <label>Channel 0 Green:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch0color[1]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh0Color([ch0color[0], newValue, ch0color[2]]);
              }}
            />
            <label>Channel 0 Blue:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch0color[2]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh0Color([ch0color[0], ch0color[1], newValue]);
              }}
            />
          </div>
          <div>
            {/* Channel 1 controls */}
            <label>Channel 1 Contrast Min:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch1Window[0]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh1Window([newValue, ch1Window[1]]);
              }}
            />
            <label>Channel 1 Contrast Max:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch1Window[1]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh1Window([ch1Window[0], newValue]);
              }}
            />
            <br />
            <label>Channel 1 Red:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch1color[0]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh1Color([newValue, ch1color[1], ch1color[2]]);
              }}
            />
            <label>Channel 1 Green:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch1color[1]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh1Color([ch1color[0], newValue, ch1color[2]]);
              }}
            />
            <label>Channel 1 Blue:</label>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={ch1color[2]}
              onChange={(e) => {
                const newValue = parseFloat(e.target.value);
                setCh1Color([ch1color[0], ch1color[1], newValue]);
              }}
            />
          </div>
          <div>
            <label>Z index:</label>
            <input
              type="range"
              min={0}
              max={236}
              step={1}
              value={zIndex}
              onChange={(e) => {
                const newValue = parseInt(e.target.value);
                setZIndex(newValue);
              }}
            />
          </div>
        </>
      ) : null}
    </div>
  );
}
