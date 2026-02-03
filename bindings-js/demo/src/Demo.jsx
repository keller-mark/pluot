import React, { useState } from "react";
import { Pluot } from "@pluot/react";

const DEMOS = {
  /*
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
  */
  layered_plot: {
    plot_type: "LayeredPlot",
    store_name: "gaussian_quantiles_store",
    plot_params: {
      layers: [
        {
          layer_type: "ZarrScatterplotLayer",
          layer_params: {
            layer_id: "layer_1",
            data_unit_mode: "Data",
            point_radius_unit_mode: "Pixels",
            point_shape_mode: "Circle",
            point_radius: 5.0,
            store_name: "gaussian_quantiles_store",
            bounds: null,

            x_key: "/n_1000000/x_coords",
            y_key: "/n_1000000/y_coords",
            color_key: "/n_1000000/class_labels",
          }
        },
        {
          layer_type: "AxisLayer",
          layer_params: {
            layer_id: "bottom_axis",
            position: "Bottom",
          }
        },
        {
          layer_type: "AxisLayer",
          layer_params: {
            layer_id: "left_axis",
            position: "Left",
          }
        },
        {
          layer_type: "AxisLayer",
          layer_params: {
            layer_id: "right_axis",
            position: "Right",
          }
        },
        {
          layer_type: "AxisLayer",
          layer_params: {
            layer_id: "top_axis",
            position: "Top",
          }
        },
        {
          layer_type: "ScatterplotLayer",
          layer_params: {
            layer_id: "layer_2",
            data_unit_mode: "Pixels",
            point_radius_unit_mode: "Pixels",
            point_shape_mode: "Square",
            point_radius: 15.0,
            store_name: "gaussian_quantiles_store",
            bounds: {
              margin_top: 0,
              margin_right: 0,
              margin_bottom: 0,
              margin_left: 0,
            },
            x_vec: [100, 100],
            y_vec: [100, 200],
            labels_vec: [0, 1],
          }
        },
        {
          layer_type: "LineLayer",
          layer_params: {
            layer_id: "layer_3",
            data_unit_mode: "Pixels",
            line_width_unit_mode: "Pixels",
            line_width: 5.0,
            store_name: "gaussian_quantiles_store",
            bounds: {
              margin_top: 0,
              margin_right: 0,
              margin_bottom: 0,
              margin_left: 0,
            },
            source_x_vec: [10, 110],
            source_y_vec: [10, 110],
            target_x_vec: [100, 210],
            target_y_vec: [100, 210],
            labels_vec: [4, 1],
          }
        },
        {
          layer_type: "TextLayer",
          layer_params: {
            layer_id: "layer_text",
            data_unit_mode: "Pixels",
            text_size_unit_mode: "Pixels",
            text_size: 25.0,
            text_align_mode: "Start",
            text_baseline_mode: "Bottom",
            bounds: null,
            /*bounds: {
              margin_top: 0,
              margin_right: 0,
              margin_bottom: 0,
              margin_left: 0,
            },*/

            x_vec: [10, 110],
            y_vec: [10, 110],
            text_vec: ["Hello", "The quick brown fox jumps over the lazy dog"],
          }
        },
        {
          layer_type: "BitmapLayer",
          layer_params: {
            layer_id: "layer_bitmap",
            data_unit_mode: "Data",

            img_size_w: 4,
            img_size_h: 4,
            img_size_c: null,
            img_size_z: null,
            img_size_t: null,
            z_index: null,
            t_index: null,
            opacity: 0.5,
            channel_settings: [
              {
                c_index: 0,
                window: [0.0, 10.0],
                color: [255.0, 0.0, 0.0],
              }
            ],

            ch0_vec: [
              300, 110, 210, 310,
              20, 120, 220, 320,
              30, 130, 230, 330,
              40, 140, 240, 340,
            ],
          }
        }
      ]
    },
  },
};

export function Demo() {
  const [currPlotId, setCurrPlotId] = useState("layered_plot");

  const plotType = DEMOS[currPlotId].plot_type;
  const plotParams = DEMOS[currPlotId].plot_params;
  const storeName = DEMOS[currPlotId].store_name;

  const [pointRadius, setPointRadius] = useState(5.0);

  const [ch0Window, setCh0Window] = useState([0.0, 0.1]);
  const [ch1Window, setCh1Window] = useState([0.0, 0.1]);
  const [ch0color, setCh0Color] = useState([1.0, 0.0, 0.0]);
  const [ch1color, setCh1Color] = useState([0.0, 1.0, 0.0]);
  const [zIndex, setZIndex] = useState(99);

  const [graphicsFormat, setGraphicsFormat] = useState("Raster");

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
      &nbsp;
      <label>Graphics Format:&nbsp;
        <input type="checkbox" checked={graphicsFormat === "Raster"} onChange={(e) => setGraphicsFormat(e.target.checked ? "Raster" : "Vector")} />
      </label>
      <Pluot
        width={800}
        height={800}
        format={graphicsFormat}
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
      {plotType === "LayeredPlot" ? (
        <div>
          <label>Point Radius (for LayeredPlot):</label>
          <input
            type="range"
            min={0.5}
            max={50.0}
            step={0.5}
            value={pointRadius}
            onChange={(e) => {
              const newValue = parseFloat(e.target.value);
              setPointRadius(newValue);
            }}
          />
          <span>
            {pointRadius}px (squares will be {pointRadius * 2.0}px * {pointRadius * 2.0}px in size)
            <div className="test-square-point" style={{ backgroundColor: 'blue', width: `${pointRadius*2}px`, height: `${pointRadius*2}px`}} />
          </span>
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
