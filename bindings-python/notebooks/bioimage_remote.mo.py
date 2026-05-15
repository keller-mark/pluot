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
    return render_to_image, zarr


@app.cell
def _():
    from obstore.store import HTTPStore
    return (HTTPStore,)


@app.cell
def _(HTTPStore, zarr):
    obs_store = HTTPStore.from_url("https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/IMG_1033-1112_asterella_gracilis.ome.zarr")
    store = zarr.storage.ObjectStore(obs_store, read_only=True)
    return (store,)


@app.cell
def _(store, zarr):
    group = zarr.open_group(store=store, mode='r')
    dict(group.attrs)
    return


@app.cell
def _():
    camera_view = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
    return (camera_view,)


@app.cell
async def _(camera_view, render_to_image, store):
    await render_to_image(
        camera_view=camera_view,
        width=600,
        height=600,
        plot_id="bioimaging",
        plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        margin_top=10,
        margin_right=10,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type = "OmeZarrMultiscaleLayer",
                  layer_params = dict(
                      layer_id= "ome_zarr_multiscale_layer",
                        target_z= 40,
                        target_t= 0,
                        channel_settings= [
                          dict(
                            c_index= 0,
                            window= [0.0, 90000.0],
                            color= [1.0, 0.0, 0.0],
                          ),
                          dict(
                            c_index= 1,
                            window= [0.0, 90000.0],
                            color= [0.0, 1.0, 0.0],
                          ),
                          dict(
                            c_index= 2,
                            window= [0.0, 90000.0],
                            color= [0.0, 0.0, 1.0],
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
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
