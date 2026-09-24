from .render import render, render_to_array, render_to_image, render_to_svg, render_to_script, extent
from .log import get_logger
import logging
# TODO: remove this, leave to user to configure.
logger = get_logger()
logger.setLevel(logging.ERROR)

__all__ = [
    "render",
    "render_to_array",
    "render_to_image",
    "render_to_svg",
    "render_to_script",
    "extent",
]
