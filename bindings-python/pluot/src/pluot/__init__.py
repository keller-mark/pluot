import importlib.util

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

if importlib.util.find_spec("pluot_widget") is not None:
    __all__ += ["PluotWasmWidget"]


# Deferred so that `import pluot` does not require the widget's built JS bundle
# (absent in editable installs without the pnpm/wasm toolchain).
def __getattr__(name):
    if name == "PluotWasmWidget":
        from pluot_widget import PluotWasmWidget
        return PluotWasmWidget
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
