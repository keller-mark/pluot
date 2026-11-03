# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "marimo",
#     "pluot[widget]",
# ]
# ///

# Launch with: uvx marimo edit --sandbox notebook.mo.py
# (--sandbox installs the dependencies listed above into an isolated environment.)
#
# Alternatively, to use an existing uv project environment:
#   uv add marimo "pluot[widget]"
#   uv run marimo edit notebook.mo.py

import marimo

app = marimo.App(width="medium")


@app.cell
def _():
    from pluot import render_to_widget
    return (render_to_widget,)


@app.cell
def _(render_to_widget):
    widget = render_to_widget(
        width=640,
        height=480,
        format="Raster",
        camera_view=[
            0.15000000596046448,
            0.0,
            0.0,
            0.0,
            0.0,
            0.15000000596046448,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0
        ],
        aspect_ratio_mode="Contain",
        aspect_ratio_alignment_mode="Center",
        view_mode="2d",
        plot_type="LayeredPlot",
        plot_params={
            "layers": [
                {
                    "layer_type": "PointLayer",
                    "layer_params": {
                        "layer_id": "pts",
                        "bounds": None,
                        "data_unit_mode_x": "Data",
                        "data_unit_mode_y": "Data",
                        "point_radius_unit_mode_x": "Pixels",
                        "point_radius_unit_mode_y": "Pixels",
                        "point_shape_mode": "Circle",
                        "model_matrix": None,
                        "point_radius": {
                            "size_mode": "UniformSize",
                            "size_params": 5.0
                        },
                        "fill_color": {
                            "color_mode": "Categorical",
                            "color_params": {
                                "codes": {
                                    "dtype": "Uint8",
                                    "values": [
                                        0,
                                        1,
                                        2,
                                        3
                                    ]
                                },
                                "colormap": "Tableau10"
                            }
                        },
                        "fill_opacity": None,
                        "stroke_width_unit_mode": "Pixels",
                        "stroke_color": None,
                        "stroke_opacity": None,
                        "stroke_width": None,
                        "position_x": {
                            "dtype": "Float32",
                            "values": [
                                0.0,
                                1.0,
                                1.0,
                                0.0
                            ]
                        },
                        "position_y": {
                            "dtype": "Float32",
                            "values": [
                                0.0,
                                0.0,
                                1.0,
                                1.0
                            ]
                        },
                        "selection_criteria": [],
                        "filtering_criteria": [],
                        "background_fill_color": None,
                        "background_stroke_color": None,
                        "background_fill_opacity": None,
                        "background_stroke_opacity": None,
                        "background_point_radius": None,
                        "background_stroke_width": None,
                        "enable_background_fill_color": True,
                        "enable_background_stroke_color": True,
                        "enable_background_fill_opacity": False,
                        "enable_background_stroke_opacity": False,
                        "enable_background_point_radius": False,
                        "enable_background_stroke_width": False
                    }
                }
            ]
        },
        plot_id="plot_1",
        stores={
            "my_store": {
                "store_type": "HttpStore",
                "store_params": {
                    "url": "https://example.com/my_store.zarr",
                    "options": None
                },
                "store_extensions": None
            }
        },
        margin_left=60.0,
    )
    widget
    return (widget,)


if __name__ == "__main__":
    app.run()
