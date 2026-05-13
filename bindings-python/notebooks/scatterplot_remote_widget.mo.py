import marimo

__generated_with = "0.18.4"
app = marimo.App(width="medium")


@app.cell
def _():
    from pluot import render_to_image, render_to_svg
    import numpy as np
    import marimo as mo
    import json
    import zarr
    return mo, render_to_image, zarr


@app.cell
def _():
    from pluot import PluotWidget
    return (PluotWidget,)


@app.cell
def _(PluotWidget):
    widget = PluotWidget(
        width=600,
        height=400,
        plot_id="my_plot",
        plot_type="LayeredPlot",
        store_name="my_store",
        plot_params={
            "layers": [
                {
                    "layer_type": "PointLayer",
                    "layer_params": {
                        "layer_id": "points",
                        "data_unit_mode_x": "Data",
                        "data_unit_mode_y": "Data",
                        "point_radius_unit_mode_x": "Pixels",
                        "point_radius_unit_mode_y": "Pixels",
                        "point_shape_mode": "Circle",
                        "point_radius": 5.0,
                        "bounds": None,
                        "position_x": [0.1, 0.3, 0.5, 0.7, 0.9],
                        "position_y": [0.5, 0.2, 0.8, 0.3, 0.6],
                        "labels_vec": [0, 1, 2, 3, 4],
                    },
                },
            ],
        },
        margin_top=20,
        margin_right=20,
        margin_bottom=40,
        margin_left=40,
    )
    widget  # display in a Jupyter cell
    return


@app.cell
def _():
    from obstore.store import HTTPStore
    return (HTTPStore,)


@app.cell
def _(HTTPStore, zarr):
    obs_store = HTTPStore.from_url("https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/gaussian_quantiles.zarr")
    store = zarr.storage.ObjectStore(obs_store, read_only=True)
    return (store,)


@app.cell
def _(store, zarr):
    arr = zarr.open_array(store=store, mode='r', path="/n_1000000/x_coords")
    arr.shape
    return


@app.cell
def _():
    camera_view = [
        0.15, 0.0, 0.0, 0.0,
        0.0, 0.15, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
    return (camera_view,)


@app.cell
async def _(camera_view, point_radius_slider, render_to_image, store):
    await render_to_image(
        camera_view=camera_view, width=600, height=600, plot_id="test_store_instance", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type="ZarrPointLayer",
                  layer_params=dict(
                    layer_id="zarr_layer",
                    data_unit_mode_x="Data",
                    data_unit_mode_y="Data",
                    point_radius_unit_mode_x="Pixels",
                    point_radius_unit_mode_y="Pixels",
                    point_shape_mode="Circle",
                    x_key="/n_1000000/x_coords",
                    y_key="/n_1000000/y_coords",
                    color_key="/n_1000000/class_labels",
                    point_radius=point_radius_slider.value,
                  )
                ),
                dict(
                    layer_type="AxisLinearLayer",
                    layer_params=dict(
                        layer_id="left_axis",
                        position="Left"
                    )
                ),
                dict(
                    layer_type="AxisLinearLayer",
                    layer_params=dict(
                        layer_id="bottom_axis",
                        position="Bottom"
                    )
                )
            ]
        ),
    )
    return


@app.cell
def _(mo):
    point_radius_slider = mo.ui.slider(start=1.0, stop=20.0, value=10.0)
    point_radius_slider
    return (point_radius_slider,)


if __name__ == "__main__":
    app.run()
