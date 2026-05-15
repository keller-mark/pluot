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
    return mo, np, render_to_image, render_to_svg, zarr


@app.cell
def _():
    from obstore.store import HTTPStore
    return (HTTPStore,)


@app.cell
def _(HTTPStore, zarr):
    obs_store = HTTPStore.from_url("https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/6001240_labels.v2.ome.zarr")
    store = zarr.storage.ObjectStore(obs_store, read_only=True)
    return (store,)


@app.cell
def _(store, zarr):
    group = zarr.open_group(store=store, mode='r')
    dict(group.attrs)
    return


@app.cell
def _():
    from pluot.viewport import (
        Bounds, ViewportParams, Margins,
        get_camera_matrix_from_bounds, get_bounds
    )
    return Bounds, Margins, ViewportParams, get_camera_matrix_from_bounds


@app.cell
def _():
    width = 800
    height = 800
    margin_left = 100
    margin_right = 10
    margin_top = 10
    margin_bottom = 100
    return height, margin_bottom, margin_left, margin_right, margin_top, width


@app.cell
def _(
    Bounds,
    Margins,
    ViewportParams,
    get_camera_matrix_from_bounds,
    height,
    margin_bottom,
    margin_left,
    margin_right,
    margin_top,
    np,
    width,
):
    # Define the viewport (e.g., 800x600 canvas with margins)
    viewport = ViewportParams(
        width=width,
        height=height,
        aspect_ratio_mode="Contain",
        aspect_ratio_alignment_mode="Center",
        margins=Margins(
            margin_top=margin_top,
            margin_right=margin_right,
            margin_bottom=margin_bottom,
            margin_left=margin_left
        ),
    )

    # Start from a default identity-like camera
    prev_camera = np.array([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ], dtype=np.float32)

    # Zoom into a specific region: x in [0.2, 0.8], y in [0.3, 0.7]
    bounds = Bounds(x_min=0.0, x_max=1.0e-4, y_min=0.0, y_max=1.0e-4)

    camera_view = list(get_camera_matrix_from_bounds(bounds, prev_camera, viewport))
    print(camera_view)
    return (camera_view,)


@app.cell
def _(mo):
    z_slider = mo.ui.slider(start=0.0, stop=235.0, value=100.0)
    z_slider
    return (z_slider,)


@app.cell
def _(mo):
    ch0_slider = mo.ui.range_slider(start=0.0, stop=2055.0, value=[0.0, 2055.0])
    ch1_slider = mo.ui.range_slider(start=0.0, stop=2055.0, value=[0.0, 2055.0])
    mo.vstack([ch0_slider, ch1_slider])
    return ch0_slider, ch1_slider


@app.cell
async def _(
    camera_view,
    ch0_slider,
    ch1_slider,
    height,
    margin_bottom,
    margin_left,
    margin_right,
    margin_top,
    render_to_image,
    store,
    width,
    z_slider,
):
    img = await render_to_image(
        camera_view=camera_view,
        width=width,
        height=height,
        plot_id="bioimaging",
        plot_type="LayeredPlot",
        margin_left=margin_left,
        margin_bottom=margin_bottom,
        margin_top=margin_top,
        margin_right=margin_right,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type = "OmeZarrMultiscaleLayer",
                  layer_params = dict(
                      layer_id= "ome_zarr_multiscale_layer",
                        target_z= int(z_slider.value),
                        target_t= 0,
                        channel_settings= [
                          dict(
                            c_index= 0,
                            window= ch0_slider.value,
                            color= [1.0, 0.0, 0.0],
                          ),
                          dict(
                            c_index= 1,
                            window= ch1_slider.value,
                            color= [0.0, 1.0, 0.0],
                          )
                        ],
                        opacity= 1.0,
                  )
                ),
                dict(
                    layer_type="AxisLinearLayer",
                    layer_params = dict(
                        layer_id="bottom_axis",
                        position="Bottom"
                    )
                ),
                dict(
                    layer_type="AxisLinearLayer",
                    layer_params = dict(
                        layer_id="left_axis",
                        position="Left"
                    )
                )
            ]
        ),
    )
    img
    return (img,)


@app.cell
def _(img):
    img.save("../pluot-figures/bioimage.png")
    return


@app.cell
async def _(
    camera_view,
    ch0_slider,
    ch1_slider,
    height,
    margin_bottom,
    margin_left,
    margin_right,
    margin_top,
    render_to_svg,
    store,
    width,
    z_slider,
):
    svg_string = await render_to_svg(
        camera_view=camera_view,
        width=width,
        height=height,
        plot_id="bioimaging",
        plot_type="LayeredPlot",
        margin_left=margin_left,
        margin_bottom=margin_bottom,
        margin_top=margin_top,
        margin_right=margin_right,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type = "OmeZarrMultiscaleLayer",
                  layer_params = dict(
                      layer_id= "ome_zarr_multiscale_layer",
                        target_z= int(z_slider.value),
                        target_t= 0,
                        channel_settings= [
                          dict(
                            c_index= 0,
                            window= ch0_slider.value,
                            color= [1.0, 0.0, 0.0],
                          ),
                          dict(
                            c_index= 1,
                            window= ch1_slider.value,
                            color= [0.0, 1.0, 0.0],
                          )
                        ],
                        opacity= 1.0,
                  )
                ),
                dict(
                    layer_type="AxisLinearLayer",
                    layer_params = dict(
                        layer_id="bottom_axis",
                        position="Bottom"
                    )
                ),
                dict(
                    layer_type="AxisLinearLayer",
                    layer_params = dict(
                        layer_id="left_axis",
                        position="Left"
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
    with open("../pluot-figures/bioimage.svg", "w") as f:
        f.write(svg_string)
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
