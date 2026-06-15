from typing import TYPE_CHECKING

from .render import render, render_to_array, render_to_image, render_to_svg
from .log import get_logger
import logging
# TODO: remove this, leave to user to configure.
logger = get_logger()
logger.setLevel(logging.ERROR)

if TYPE_CHECKING:
    from .widget_py import PluotPyWidget  # noqa: F401
    from .widget_wasm import PluotWasmWidget  # noqa: F401

__all__ = [
    "render",
    "render_to_array",
    "render_to_image",
    "render_to_svg",
    "PluotPyWidget",
    "PluotWasmWidget",
]


def __getattr__(name: str):
    if name == "PluotPyWidget":
        try:
            from .widget_py import PluotPyWidget
        except ImportError as e:
            raise ImportError(
                "PluotPyWidget requires the 'widget' extra. "
                "Install with: pip install pluot[widget]"
            ) from e
        return PluotPyWidget
    if name == "PluotWasmWidget":
        try:
            from .widget_wasm import PluotWasmWidget
        except ImportError as e:
            raise ImportError(
                "PluotWasmWidget requires the 'widget' extra. "
                "Install with: pip install pluot[widget]"
            ) from e
        return PluotWasmWidget
    raise AttributeError(f"module 'pluot' has no attribute {name!r}")
