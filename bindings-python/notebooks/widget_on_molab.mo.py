# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "marimo>=0.25.1",
#     "zarr>=3",
#     "pluot_widget"
# ]
# ///
#

import marimo

__generated_with = "0.25.1"
app = marimo.App(width="medium")


@app.cell
def _():
    from pluot_widget import PluotWasmWidget
    import numpy as np
    import marimo as mo
    import json
    import zarr

    return PluotWasmWidget, mo


@app.cell
def _():
    from fsspec.implementations.asyn_wrapper import AsyncFileSystemWrapper
    from fsspec.implementations.http_sync import HTTPFileSystem
    from zarr.storage import FsspecStore

    url = "https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/gaussian_quantiles.zarr"
    fs = AsyncFileSystemWrapper(HTTPFileSystem(), asynchronous=True)
    store = FsspecStore(fs=fs, path=url, read_only=True)
    return (store,)


@app.cell
def _():
    camera_view = [
        0.15, 0.0, 0.0, 0.0,
        0.0, 0.15, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
    return (camera_view,)


@app.function
def make_plot_params(point_radius):
    return dict(
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
                x_key="/n_1000/x_coords",
                y_key="/n_1000/y_coords",
                color_key="/n_1000/class_labels",
                point_radius=point_radius,
                point_opacity=1.0
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
    )


@app.cell
def _(mo):
    point_radius_slider = mo.ui.slider(start=0.1, stop=5.0, value=0.5, step=0.1)
    point_radius_slider
    return (point_radius_slider,)


@app.cell
def _(PluotWasmWidget, camera_view, store):
    # Must not depend on UI element values, or marimo re-runs this cell and
    # re-creates the widget. Updates are pushed via traitlets in the cell below.
    widget2 = PluotWasmWidget(
        camera_view=camera_view, width=1000, height=600, plot_id="test_store_instance", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        store=store,
        store_name = "my_store",
        plot_params=make_plot_params(0.5),
    )
    widget2
    return (widget2,)


@app.cell
def _(point_radius_slider, widget2):
    widget2.plot_params = make_plot_params(point_radius_slider.value)
    return


@app.cell
def _(widget2):
    widget2.width = 600
    return


@app.cell
def _(mo, widget2):
    get_camera, set_camera = mo.state(widget2.camera_view)
    widget2.observe(lambda _: set_camera(widget2.camera_view), names=["camera_view"])
    return (get_camera,)


@app.cell
def _(get_camera, mo):
    curr_camera_view = get_camera()
    mo.md(f"Camera is at **{curr_camera_view}s**")
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
