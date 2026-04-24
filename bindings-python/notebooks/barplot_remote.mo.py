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
    obs_store = HTTPStore.from_url("https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/wheat.zarr")
    store = zarr.storage.ObjectStore(obs_store, read_only=True)
    return (store,)


@app.cell
def _():
    camera_view = [
        0.15, 0.0, 0.0, 0.0,
        0.0, 0.01, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, -1.0, 0.0, 1.0,
    ]
    return (camera_view,)


@app.cell
async def _(camera_view, render_to_image, store):
    await render_to_image(
        camera_view=camera_view, width=800, height=600, plot_id="test_barplot_layer", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        margin_top=10,
        margin_right=10,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type = "ZarrBarPlotLayer",
                  layer_params = dict(
                      layer_id= "barplot_layer",
            bounds= None,

            orientation= "Vertical",
            identifier_key= "/year",
            quantity_key= "/wheat",
        
            fill_color_mode="Categorical",
                  )
                ),
            ]
        ),
    )
    return


@app.cell
async def _(camera_view, render_to_svg, store):
    svg_string = await render_to_svg(
        camera_view=camera_view, width=800, height=600, plot_id="test_barplot_layer", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        margin_top=10,
        margin_right=10,
        store=store,
        plot_params=dict(
            layers=[
                dict(
                  layer_type = "ZarrBarPlotLayer",
                  layer_params = dict(
                      layer_id= "barplot_layer",
            bounds= None,

            orientation= "Vertical",
            identifier_key= "/year",
            quantity_key= "/wheat",
        
            fill_color_mode="Categorical",
                  )
                ),
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
    with open("../pluot-figures/barplot.svg", "w") as f:
       f.write(svg_string)
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
