from pluot_core.font import register_font
from pluot_core.viewport import Bounds, Margins, ViewportParams, get_bounds, get_camera_matrix_from_bounds
from pluot_core.zarr import register_store_extension

__all__ = [
    "register_font",
    "register_store_extension",
    "Bounds",
    "Margins",
    "ViewportParams",
    "get_bounds",
    "get_camera_matrix_from_bounds",
]

try:
    from pluot_bound import render, render_to_array, render_to_image, render_to_svg, render_to_script, extent
    __all__ += ["render", "render_to_array", "render_to_image", "render_to_svg", "render_to_script", "extent"]
except ModuleNotFoundError as e:
    if e.name != "pluot_bound":
        raise

try:
    from pluot_widget import PluotWasmWidget
    __all__ += ["PluotWasmWidget"]
except ModuleNotFoundError as e:
    if e.name != "pluot_widget":
        raise
