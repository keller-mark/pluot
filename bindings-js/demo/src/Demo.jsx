import React, { useState } from "react";
import { Pluot } from "@pluot/react";

const DEMOS = {
  layered_plot: {
    plot_type: "LayeredPlot",
    store_name: "gaussian_quantiles_store",
    plot_params: {
      layers: [
        {
          layer_type: "ZarrPointLayer",
          layer_params: {
            layer_id: "layer_1",
            data_unit_mode_x: "Data",
            data_unit_mode_y: "Data",
            point_radius_unit_mode_x: "Pixels",
            point_radius_unit_mode_y: "Pixels",
            point_shape_mode: "Circle",
            point_radius: 5.0,
            store_name: "gaussian_quantiles_store",
            bounds: null,

            x_key: "/n_1000/x_coords",
            y_key: "/n_1000/y_coords",
            color_key: "/n_1000/class_labels",
          }
        },
        {
          layer_type: "AxisLinearLayer",
          layer_params: {
            layer_id: "bottom_axis",
            position: "Bottom",
          }
        },
        {
          layer_type: "AxisLinearLayer",
          layer_params: {
            layer_id: "left_axis",
            position: "Left",
          }
        },
        {
          layer_type: "AxisLinearLayer",
          layer_params: {
            layer_id: "right_axis",
            position: "Right",
          }
        },
        {
          layer_type: "AxisBandLayer",
          layer_params: {
            layer_id: "top_axis",
            position: "Top",
            domain: ["One", "Two", "Three", "Four", "Five"]
          }
        },
        {
          layer_type: "PointLayer",
          layer_params: {
            layer_id: "layer_2",
            data_unit_mode_x: "Pixels",
            data_unit_mode_y: "Pixels",
            point_radius_unit_mode_x: "Pixels",
            point_radius_unit_mode_y: "Pixels",
            point_shape_mode: "Square",
            point_radius: 15.0,
            store_name: "gaussian_quantiles_store",
            bounds: {
              margin_top: 0,
              margin_right: 0,
              margin_bottom: 0,
              margin_left: 0,
            },
            position_x: [100, 100],
            position_y: [100, 200],
            labels_vec: [0, 1],
          }
        },
        {
          layer_type: "LineLayer",
          layer_params: {
            layer_id: "layer_3",
            data_unit_mode_x: "Pixels",
            data_unit_mode_y: "Pixels",
            line_width_unit_mode: "Pixels",
            line_width: 5.0,
            store_name: "gaussian_quantiles_store",
            bounds: {
              margin_top: 0,
              margin_right: 0,
              margin_bottom: 0,
              margin_left: 0,
            },
            source_position_x: [10, 110],
            source_position_y: [10, 110],
            target_position_x: [100, 210],
            target_position_y: [100, 210],
            labels_vec: [4, 1],
          }
        },
        {
          layer_type: "TextLayer",
          layer_params: {
            layer_id: "layer_text",
            data_unit_mode_x: "Pixels",
            data_unit_mode_y: "Pixels",
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

            position_x: [10, 110],
            position_y: [10, 110],
            text_vec: ["Hello", "The quick brown fox jumps over the lazy dog"],
          }
        },
        {
          layer_type: "BitmapLayer",
          layer_params: {
            layer_id: "layer_bitmap",
            data_unit_mode_x: "Data",
            data_unit_mode_y: "Data",
            pixel_offset: [1, 1],

            dimension_order: "CYX",
            shape: [2, 4, 4],
            opacity: 0.5,
            channel_settings: [
              {
                window: [0.0, 500.0],
                color: [1.0, 0.0, 0.0],
              },
              {
                window: [0.0, 500.0],
                color: [0.0, 0.0, 1.0],
              }
            ],

            data: { Uint16: [
              0, 110, 210, 310,
              20, 120, 220, 320,
              30, 130, 230, 330,
              40, 140, 240, 340,
              300, 110, 210, 310,
              20, 120, 220, 320,
              30, 130, 230, 330,
              40, 140, 240, 0,
            ]},
          }
        },
        {
          layer_type: "RectLayer",
          layer_params: {
            layer_id: "rect_layer",
            data_unit_mode_x: "Data",
            data_unit_mode_y: "Data",
            stroke_width_unit_mode: "Pixels",
            stroke_width: 5.0,
            position_x0: [1],
            position_y0: [1],
            position_x1: [2],
            position_y1: [3],
            labels_vec: [4],
          }
        },
        /*{
          layer_type: "TileLayer",
          layer_params: {
            layer_id: "tile_layer",
            tile_size: 4,
          }
        },*/
        {
          layer_type: "MultiscaleLayer",
          layer_params: {
            layer_id: "multiscale_layer",
            resolution_levels: [
              {
                shape: [200, 200],
                chunk_shape: [50, 50],
                scale: [1.0, 1.0],
              },
              {
                shape: [100, 100],
                chunk_shape: [50, 50],
                scale: [2.0, 2.0],
              }
            ]
          }
        },
        {
          layer_type: "OmeZarrMultiscaleLayer",
          layer_params: {
            layer_id: "ome_zarr_multiscale_layer",
            store_name: "ome_ngff_2",
            target_z: 40,
            target_t: 0,
            channel_settings: [
              {
                c_index: 0,
                window: [0.0, 90000.0],
                color: [1.0, 0.0, 0.0],
              },
              {
                c_index: 1,
                window: [0.0, 90000.0],
                color: [0.0, 1.0, 0.0],
              },
              {
                c_index: 2,
                window: [0.0, 90000.0],
                color: [0.0, 0.0, 1.0],
              }
            ],
            opacity: 1.0,
          },
        },
        {
          layer_type: "ComputeLayer",
          layer_params: {
            layer_id: "compute_layer",
          }
        }
      ]
    },
  },
  three_d_plot: {
    plot_type: "LayeredPlot",
    store_name: "gaussian_quantiles_store",
    plot_params: {
      layers: [
        {
          layer_type: "ZarrPoint3dLayer",
          layer_params: {
            layer_id: "layer_1",
            data_unit_mode_x: "Data",
            data_unit_mode_y: "Data",
            point_radius_unit_mode_x: "Pixels",
            point_radius_unit_mode_y: "Pixels",
            point_shape_mode: "Circle",
            point_radius: 5.0,
            store_name: "gaussian_quantiles_store",
            bounds: {
              margin_top: 0,
              margin_right: 0,
              margin_bottom: 0,
              margin_left: 0,
            },

            x_key: "/n_1000/x_coords",
            y_key: "/n_1000/y_coords",
            z_key: "/n_1000/z_coords",
            color_key: "/n_1000/class_labels",
          }
        }
      ]
    },
  },
  bar_plot: {
    plot_type: "LayeredPlot",
    store_name: "wheat",
    plot_params: {
      layers: [
        {
          layer_type: "ZarrBarPlotLayer",
          layer_params: {
            layer_id: "layer_1",
            store_name: "wheat",
            bounds: null,
            orientation: "Vertical",
            identifier_key: "/year",
            quantity_key: "/wheat",
          }
        },
      ]
    }
  },
  bar_plot_2: {
    plot_type: "LayeredPlot",
    store_name: "wheat",
    plot_params: {
      layers: [
        {
          layer_type: "BarPlotLayer",
          layer_params: {
            layer_id: "layer_1",
            bounds: null,
            orientation: "Horizontal",
            data_unit_mode_for_identifier_dim: "Pixels",
            data_unit_mode_for_quantity_dim: "Data",

            identifier: ["One", "Two", "Three"],
            quantity: [10.0, 20.0, 30.0],
          }
        },
      ]
    }
  }
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
        viewMode={currPlotId === "three_d_plot" ? "3d" : "2d"}
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
      {false && plotType === "LayeredPlot" ? (
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
