from typing import TYPE_CHECKING

from .render import render, render_to_array, render_to_image, render_to_svg
from .log import get_logger
import logging
# TODO: remove this, leave to user to configure.
logger = get_logger()
logger.setLevel(logging.ERROR)

if TYPE_CHECKING:
    from .widget_py import PluotWidget  # noqa: F401

__all__ = [
    "render",
    "render_to_array",
    "render_to_image",
    "render_to_svg",
    "PluotWidget",
]


def __getattr__(name: str):
    if name == "PluotWidget":
        try:
            from .widget_py import PluotWidget
        except ImportError as e:
            raise ImportError(
                "PluotWidget requires the 'widget' extra. "
                "Install with: pip install pluot[widget]"
            ) from e
        return PluotWidget
    raise AttributeError(f"module 'pluot' has no attribute {name!r}")
