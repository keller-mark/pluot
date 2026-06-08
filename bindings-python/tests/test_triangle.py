import pytest

from pluot import render, render_to_array, render_to_svg

camera_view = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
]

basic_plot_kwargs = dict(
    camera_view=camera_view,
    width=100,
    height=100,
    plot_id="test",
    plot_type="LayeredPlot",
    store_name="my_store",
    plot_params=dict(
        layers=[
            dict(
                layer_type="PointLayer",
                    layer_params=dict(
                    layer_id="scatter_layer",
                    data_unit_mode_x="Data",
                    data_unit_mode_y="Data",
                    point_radius_unit_mode_x="Pixels",
                    point_radius_unit_mode_y="Pixels",
                    point_shape_mode="Square",
                    point_radius=25.0,
                    bounds=None,
                    position_x=[0, 1, 0, 1],
                    position_y=[0, 0, 1, 1],
                    labels_vec=[0, 1, 2, 3],
                )
            ),
        ]
    ),
)

@pytest.mark.asyncio
async def test_render_triangle():
    result = await render(**basic_plot_kwargs)
    assert result is not None
    assert len(result) == (100 * 100 * 4) + 1  # RGBA for each pixel, plus one extra value
    assert sum(result) == 1412500  # Expected sum for a triangle rendering

@pytest.mark.asyncio
async def test_render_to_array():
    arr = await render_to_array(**basic_plot_kwargs)
    assert arr.shape == (100, 100, 4)
    assert arr.dtype == 'uint8'
    assert arr.sum() == 1412500

@pytest.mark.asyncio
async def test_render_to_svg():
    svg_str = await render_to_svg(**basic_plot_kwargs)
    assert len(svg_str) == 635
