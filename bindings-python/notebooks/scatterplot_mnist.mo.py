import marimo

__generated_with = "0.18.4"
app = marimo.App(width="medium")


@app.cell
def _():
    from pluot import render_to_image, render_to_svg
    import numpy as np
    import pandas as pd
    import marimo as mo
    import json
    import zarr
    return mo, np, pd, render_to_image, render_to_svg


@app.cell
def _():
    import umap
    from sklearn import datasets
    return datasets, umap


@app.cell
def _(datasets):
    mnist = datasets.fetch_openml("mnist_784")
    return (mnist,)


@app.cell
def _(mnist, umap):
    mapper = umap.UMAP(random_state=123).fit(mnist.data)
    dens_mapper = umap.UMAP(densmap=True, random_state=123, n_neighbors=30).fit(mnist.data)
    return dens_mapper, mapper


@app.cell
def _(dens_mapper, mapper, mnist, np, pd):
    # convert the transformed data into dataframe
    umap_df = pd.DataFrame(np.column_stack((mapper.embedding_, mnist.target)), columns=['X', 'Y', "Targets"])
    densmap_df = pd.DataFrame(np.column_stack((dens_mapper.embedding_, mnist.target)), columns=['X', 'Y', "Targets"])

    umap_df["Targets"] = umap_df["Targets"].astype(int)
    densmap_df["Targets"] = densmap_df["Targets"].astype(int)
    return (umap_df,)


@app.cell
def _():
    camera_view = [
        0.04, 0.0, 0.0, 0.0,
        0.0, 0.04, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        -0.4, -0.4, 0.0, 1.0,
    ]
    return (camera_view,)


@app.cell
async def _(camera_view, render_to_image, umap_df):
    await render_to_image(
        camera_view=camera_view, width=600, height=600, plot_id="test_store_instance", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        margin_top=10,
        margin_right=10,
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
                    x_arr=umap_df["X"].values.astype('<f8'),
                    y_arr=umap_df["Y"].values.astype('<f8'),
                    color_arr=umap_df["Targets"].values.astype('<i8'),
                    point_radius=2.0,
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
                        bounds=dict(margin_top=0, margin_left=0, margin_right=0, margin_bottom=0),
                        position_x=[100 + (600-100-10)/2],
                        position_y=[60],
                        text_vec=["UMAP_1"]
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
                        bounds=dict(margin_top=0, margin_left=0, margin_right=0, margin_bottom=0),
                        position_y=[100 + (600-100-10)/2],
                        position_x=[60],
                        text_vec=["UMAP_2"]
                    )
                )
            ]
        ),
    )
    return


@app.cell
async def _(camera_view, mo, render_to_svg, umap_df):
    mo.Html(await render_to_svg(
        camera_view=camera_view, width=600, height=600, plot_id="test_store_instance", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        margin_top=10,
        margin_right=10,
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
                    x_arr=umap_df["X"].values.astype('<f8'),
                    y_arr=umap_df["Y"].values.astype('<f8'),
                    color_arr=umap_df["Targets"].values.astype('<i8'),
                    point_radius=2.0,
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
                        bounds=dict(margin_top=0, margin_left=0, margin_right=0, margin_bottom=0),
                        position_x=[100 + (600-100-10)/2],
                        position_y=[60],
                        text_vec=["UMAP_1"]
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
                        bounds=dict(margin_top=0, margin_left=0, margin_right=0, margin_bottom=0),
                        position_y=[100 + (600-100-10)/2],
                        position_x=[60],
                        text_vec=["UMAP_2"]
                    )
                )
            ]
        ),
    )
    )
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
