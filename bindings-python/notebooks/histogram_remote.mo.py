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
    return mo, render_to_image, render_to_svg, zarr


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
        0.0, 0.00001, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, -1.0, 0.0, 1.0,
    ]
    return (camera_view,)


@app.cell
async def _(camera_view, num_bins_slider, render_to_image, store):
    await render_to_image(
        camera_view=camera_view, width=600, height=600, plot_id="test_histogram_layer", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        margin_top=10,
        margin_right=10,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type = "ZarrHistogramLayer",
                  layer_params = dict(
                      layer_id= "histogram_layer",
                        bounds= None,
                        orientation="Vertical",
                        data_key= "/n_1000000/x_coords",
                        num_bins= int(num_bins_slider.value),
                        cache_data=False,
                        fill_color= None,
                  )
                ),
            ]
        ),
    )
    return


@app.cell
def _(mo):
    num_bins_slider = mo.ui.slider(start=10.0, stop=100.0, value=50.0)
    num_bins_slider
    return (num_bins_slider,)


@app.cell
def _():
    width = 1200
    height = 600
    return height, width


@app.cell
async def _(camera_view, height, num_bins_slider, render_to_svg, store, width):
    svg_string = await render_to_svg(
        camera_view=camera_view, width=width, height=height, plot_id="test_histogram_layer", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        margin_top=10,
        margin_right=10,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type = "ZarrHistogramLayer",
                  layer_params = dict(
                      layer_id= "histogram_layer",
                        bounds= None,
                        orientation="Vertical",
                        data_key= "/n_1000000/x_coords",
                        num_bins= int(num_bins_slider.value),
                        cache_data=False,
                        fill_color=None,
                  )
                ),
                dict(
                    layer_type="TextLayer",
                    layer_params=dict(
                        layer_id="text_label_x",
                        data_unit_mode_x="Pixels",
                        data_unit_mode_y="Pixels",
                        text_size_unit_mode="Pixels",
                        text_size=15.0,
                        text_align_mode="Middle",
                        text_baseline_mode="Middle",
                        font_family=None,
                        font_weight="Normal",
                        font_style="Normal",
                        bounds=dict(margin_top=0, margin_left=0, margin_right=0, margin_bottom=0),
                        position_x=dict(dtype="Float32", values=[100 + (width-100-10)/2]),
                        position_y=dict(dtype="Float32", values=[15]),
                        text_vec=["Bin"]
                    )
                ),
                dict(
                    layer_type="TextLayer",
                    layer_params=dict(
                        layer_id="text_label_y",
                        data_unit_mode_x="Pixels",
                        data_unit_mode_y="Pixels",
                        text_size_unit_mode="Pixels",
                        text_size=15.0,
                        text_align_mode="Middle",
                        text_baseline_mode="Middle",
                        text_rotation=-90.0,
                        font_family=None,
                        font_weight="Normal",
                        font_style="Normal",
                        bounds=dict(margin_top=0, margin_left=0, margin_right=0, margin_bottom=0),
                        position_y=dict(dtype="Float32", values=[100 + (height-100-10)/2]),
                        position_x=dict(dtype="Float32", values=[25]),
                        text_vec=["Count"]
                    )
                )
            ]
        ),
    )
    return (svg_string,)


@app.cell
def _(mo, svg_string):
    mo.Html(svg_string)
    return


@app.cell
def _(svg_string):
    with open("../pluot-figures/histogram.svg", "w") as f:
       f.write(svg_string)
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
