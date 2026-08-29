# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "pluot",
# ]
# ///
from pluot import render_to_image
import asyncio

async def main():
    plot = await render_to_image(
        schema_version=None,
        width=640,
        height=480,
        device_pixel_ratio=1.0,
        camera_view=[
            0.15000000596046448,
            0.0,
            0.0,
            0.0,
            0.0,
            0.15000000596046448,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0
        ],
        aspect_ratio_mode="Contain",
        aspect_ratio_alignment_mode="Center",
        view_mode="2d",
        plot_type="LayeredPlot",
        plot_params={
            "layers": [
                {
                    "layer_type": "PointLayer",
                    "layer_params": {
                        "layer_id": "pts",
                        "bounds": None,
                        "data_unit_mode_x": "Data",
                        "data_unit_mode_y": "Data",
                        "point_radius_unit_mode_x": "Pixels",
                        "point_radius_unit_mode_y": "Pixels",
                        "point_shape_mode": "Circle",
                        "model_matrix": None,
                        "point_radius": {
                            "size_mode": "UniformSize",
                            "size_params": 5.0
                        },
                        "fill_color": {
                            "color_mode": "Categorical",
                            "color_params": {
                                "codes": {
                                    "dtype": "Uint8",
                                    "values": [
                                        0,
                                        1,
                                        2,
                                        3
                                    ]
                                },
                                "colormap": "Tableau10"
                            }
                        },
                        "fill_opacity": None,
                        "stroke_width_unit_mode": "Pixels",
                        "stroke_color": None,
                        "stroke_opacity": None,
                        "stroke_width": None,
                        "position_x": {
                            "dtype": "Float32",
                            "values": [
                                0.0,
                                1.0,
                                1.0,
                                0.0
                            ]
                        },
                        "position_y": {
                            "dtype": "Float32",
                            "values": [
                                0.0,
                                0.0,
                                1.0,
                                1.0
                            ]
                        },
                        "selection_criteria": [],
                        "filtering_criteria": [],
                        "background_fill_color": None,
                        "background_stroke_color": None,
                        "background_fill_opacity": None,
                        "background_stroke_opacity": None,
                        "background_point_radius": None,
                        "background_stroke_width": None,
                        "enable_background_fill_color": True,
                        "enable_background_stroke_color": True,
                        "enable_background_fill_opacity": False,
                        "enable_background_stroke_opacity": False,
                        "enable_background_point_radius": False,
                        "enable_background_stroke_width": False
                    }
                }
            ]
        },
        plot_id="plot_1",
        stores={
            "my_store": {
                "store_type": "HttpStore",
                "store_params": {
                    "url": "https://example.com/my_store.zarr",
                    "options": None
                },
                "store_extensions": None
            }
        },
        wait_for_store_gets=True,
        timeout=None,
        cache_enabled=True,
        svg_compression_enabled=False,
        svg_include_document=True,
        margin_left=60.0,
        margin_right=None,
        margin_top=None,
        margin_bottom=None,
        pickable=False,
        render_backend=None,
        compute_backend=None,
    )
    #plot.save("my_plot.png")
    return plot
if __name__ == "__main__":
    asyncio.run(main())
