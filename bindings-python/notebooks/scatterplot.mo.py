import marimo

__generated_with = "0.18.4"
app = marimo.App(width="medium")


@app.cell
def _():
    from pluot import render_to_image, render_to_svg
    import numpy as np
    import marimo as mo
    import json
    return mo, np, render_to_image, render_to_svg


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
async def _(camera_view, render_to_image):
    await render_to_image(
        camera_view=camera_view,
        width=500,
        height=500,
        margin_left=60,
        plot_id="test",
        plot_type="LayeredPlot",
        store_name="my_store",
        plot_params=dict(
            #x_key="/n_100000/x_coords",
            #y_key="/n_100000/y_coords",
            #color_key="/n_1000000/class_labels",
            #point_radius=5.0
            layers=[
                dict(
                    layer_type="PointLayer",
                      layer_params=dict(
                        layer_id="layer_2",
                        data_unit_mode_x="Pixels",
                        data_unit_mode_y="Pixels",
                        point_radius_unit_mode_x="Pixels",
                        point_radius_unit_mode_y="Pixels",
                        point_shape_mode="Square",
                        point_radius={ "size_mode": "UniformSize", "size_params": 15.0 },
                        store_name="my_store",
                        bounds=dict(
                          margin_top= 0,
                          margin_right=0,
                          margin_bottom=0,
                          margin_left=0,
                        ),
                        position_x={"dtype": "Float32", "values": [100, 100, 400, 400]},
                        position_y={"dtype": "Float32", "values": [100, 400, 100, 400]},
                        fill_color={ "color_mode": "Categorical", "color_params": {
                            "codes": { "dtype": "Uint8", "values": [0, 1, 2, 3] },
                            "colormap": "Tableau10"
                        } },
                      )
                ),
                dict(
                    layer_type="AxisLinearLayer",
                    layer_params=dict(
                        layer_id="left_axis",
                        position="Left"
                    )
                )
            ]
        ),
    )
    return


@app.cell
def _(np):
    x_arr = ((np.random.rand(500) - 0.5) * 10.0).astype('<f8')
    y_arr = ((np.random.rand(500) - 0.5) * 10.0).astype('<f8')
    color_arr = np.array(
      [5, 6, 7, 6, 8] * 100
    ).astype('<i8')
    return color_arr, x_arr, y_arr


@app.cell
def _(mo):
    point_radius_slider = mo.ui.slider(start=1.0, stop=20.0, value=10.0)
    point_radius_slider
    return (point_radius_slider,)


@app.cell
async def _(
    camera_view,
    color_arr,
    point_radius_slider,
    render_to_image,
    x_arr,
    y_arr,
):
    await render_to_image(
        camera_view=camera_view, width=600, height=600, plot_id="test", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        plot_params=dict(
            layers=[
                dict(
                  layer_type="PointLayer",
                  layer_params=dict(
                    layer_id="zarr_layer",
                    data_unit_mode_x="Data",
                    data_unit_mode_y="Data",
                    point_radius_unit_mode_x="Pixels",
                    point_radius_unit_mode_y="Pixels",
                    point_shape_mode="Circle",
                    point_radius={ "size_mode": "UniformSize", "size_params": point_radius_slider.value },
                    position_x={"dtype": "Float32", "values": x_arr.tolist() },
                    position_y={"dtype": "Float32", "values": y_arr.tolist()},
                    fill_color={ "color_mode": "Categorical", "color_params": {
                        "codes": { "dtype": "Uint8", "values": color_arr.tolist() },
                        "colormap": "Tableau10"
                    } },
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
async def _(
    camera_view,
    color_arr,
    mo,
    point_radius_slider,
    render_to_svg,
    x_arr,
    y_arr,
):
    mo.Html(await render_to_svg(
        camera_view=camera_view, width=600, height=600, plot_id="test", plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        plot_params=dict(
            layers=[
                dict(
                  layer_type="PointLayer",
                  layer_params=dict(
                    layer_id="zarr_layer",
                    data_unit_mode_x="Data",
                    data_unit_mode_y="Data",
                    point_radius_unit_mode_x="Pixels",
                    point_radius_unit_mode_y="Pixels",
                    point_shape_mode="Circle",
                    point_radius={ "size_mode": "UniformSize", "size_params": point_radius_slider.value },
                    position_x={"dtype": "Float32", "values": x_arr.tolist() },
                    position_y={"dtype": "Float32", "values": y_arr.tolist()},
                    fill_color={ "color_mode": "Categorical", "color_params": {
                        "codes": { "dtype": "Uint8", "values": color_arr.tolist() },
                        "colormap": "Tableau10"
                    } },
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
    ))
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
