import marimo

__generated_with = "0.18.4"
app = marimo.App(width="medium")


@app.cell
def _():
    import marimo as mo
    from pluot import render_to_image, render_to_svg
    import json
    import pandas as pd
    import numpy as np
    return mo, np, pd, render_to_image, render_to_svg


@app.cell
def _():
    from pluot.viewport import (
        Bounds, ViewportParams, Margins,
        get_camera_matrix_from_bounds, get_bounds,
    )
    return Bounds, Margins, ViewportParams, get_camera_matrix_from_bounds


@app.cell
def _():
    json_url = "https://raw.githubusercontent.com/keller-mark/vueplotlib/refs/heads/master/examples-src/data/rainfall.json"
    return (json_url,)


@app.cell
def _(json_url, pd):
    df = pd.read_json(json_url)
    return (df,)


@app.cell
def _(df):
    df.head()
    return


@app.cell
def _():
    # Reference: https://github.com/keller-mark/vueplotlib/blob/master/src/scales/GenomeScale.js

    CHROMOSOMES = [
        '1',
        '2', 
        '3', 
        '4', 
        '5', 
        '6', 
        '7', 
        '8', 
        '9', 
        '10', 
        '11', 
        '12', 
        '13', 
        '14', 
        '15', 
        '16', 
        '17', 
        '18', 
        '19', 
        '20', 
        '21', 
        '22', 
        'X', 
        'Y', 
        'M'
    ]

    CHROMOSOME_LENGTHS = {
        '1': 249250621,
        '2': 243199373,
        '3': 198022430,
        '4': 191154276,
        '5': 180915260,
        '6': 171115067,
        '7': 159138663,
        '8': 146364022,
        '9': 141213431,
        '10': 135534747,
        '11': 135006516,
        '12': 133851895,
        '13': 115169878,
        '14': 107349540,
        '15': 102531392,
        '16': 90354753,
        '17': 81195210,
        '18': 78077248,
        '19': 59128983,
        '20': 63025520,
        '21': 48129895,
        '22': 51304566,
        'X': 155270560,
        'Y': 59373566,
        'M': 16571
    }
    return CHROMOSOMES, CHROMOSOME_LENGTHS


@app.cell
def _(CHROMOSOMES, CHROMOSOME_LENGTHS):
    chr_domains = [ (0, CHROMOSOME_LENGTHS[chr_name]) for chr_name in CHROMOSOMES ]
    genome_len = sum(CHROMOSOME_LENGTHS.values())
    return (genome_len,)


@app.cell
def _(CHROMOSOMES, CHROMOSOME_LENGTHS):
    def chr_pos_to_genome_pos(chr_name, chr_pos):
        # Return the absolute position.
        chr_idx = CHROMOSOMES.index(chr_name)
        result = 0
        for curr_chr_idx, curr_chr_name in enumerate(CHROMOSOMES):
            if curr_chr_idx == chr_idx:
                result += chr_pos
                return result
            elif curr_chr_idx < chr_idx:
                result += CHROMOSOME_LENGTHS[curr_chr_name]
            else:
                return result
        return result
    return (chr_pos_to_genome_pos,)


@app.cell
def _(chr_pos_to_genome_pos):
    assert chr_pos_to_genome_pos('1', 10) == 10
    assert chr_pos_to_genome_pos('2', 10) == (249250621 + 10)
    assert chr_pos_to_genome_pos('3', 10) == (249250621 + 243199373 + 10)
    return


@app.cell
def _(chr_pos_to_genome_pos, df):
    df["cumpos"] = df.apply(lambda row: chr_pos_to_genome_pos(row["chr"], row["pos"]), axis='columns')
    return


@app.cell
def _(df):
    df["cat_coarse"] = df["cat"].apply(lambda val: val[2:5])
    return


@app.cell
def _(df):
    unique_cats = df["cat_coarse"].unique().tolist()
    df["cat_coarse_idx"] = df["cat_coarse"].apply(lambda val: unique_cats.index(val))
    return


@app.cell
def _(df):
    y_max_val = int(df["mut_dist"].max())
    return (y_max_val,)


@app.cell
def _(df):
    df.head()
    return


@app.cell
def _(df):
    x_arr = df["cumpos"].values.astype('<f8')
    y_arr = df["mut_dist"].values.astype('<f8')
    color_arr = df["cat_coarse_idx"].values.astype('<i8')
    return color_arr, x_arr, y_arr


@app.cell
def _():
    camera_view = [
        0.15, 0.0, 0.0, 0.0,
        0.0, 0.15, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
    return


@app.cell
def _(mo, y_max_val):
    y_max_slider = mo.ui.slider(start=1.0, stop=y_max_val, value=y_max_val)
    y_max_slider
    return (y_max_slider,)


@app.cell
def _():
    width = 1500
    height = 600
    return height, width


@app.cell
def _(
    Bounds,
    Margins,
    ViewportParams,
    genome_len,
    get_camera_matrix_from_bounds,
    height,
    np,
    width,
    y_max_slider,
):
    # Define viewport: 500x500 with margins and "Contain" aspect ratio mode
    viewport = ViewportParams(
        width=width,
        height=height,
        aspect_ratio_mode="Ignore",
        aspect_ratio_alignment_mode="Start",
        margins=Margins(margin_top=0, margin_right=0, margin_bottom=100, margin_left=100),
    )

    # Start from a default identity-ish camera
    prev_camera = np.array([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ], dtype=np.float32)

    camera = get_camera_matrix_from_bounds(
        bounds=Bounds(x_min=0.0, x_max=genome_len / 3.0, y_min=0.0, y_max=y_max_slider.value),
        prev_camera_matrix=prev_camera,
        viewport_params=viewport,
    )
    return (camera,)


@app.cell
def _(mo):
    point_radius_slider = mo.ui.slider(start=1.0, stop=10.0, value=5.0)
    point_radius_slider
    return (point_radius_slider,)


@app.cell
async def _(
    camera,
    color_arr,
    height,
    point_radius_slider,
    render_to_image,
    width,
    x_arr,
    y_arr,
):
    await render_to_image(
        camera_view=camera.tolist(),
        width=width,
        height=height,
        plot_id="test",
        plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        aspect_ratio_mode="Ignore",
        aspect_ratio_alignment_mode="Start",
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
    camera,
    color_arr,
    height,
    mo,
    point_radius_slider,
    render_to_svg,
    width,
    x_arr,
    y_arr,
):
    mo.Html(await render_to_svg(
        camera_view=camera.tolist(),
        width=width,
        height=height,
        plot_id="test",
        plot_type="LayeredPlot",
        margin_left=100,
        margin_bottom=100,
        aspect_ratio_mode="Ignore",
        aspect_ratio_alignment_mode="Start",
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
