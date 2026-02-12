
from .render import render, render_to_array, render_to_image
from .log import get_logger
import logging
# TODO: remove this, leave to user to configure.
logger = get_logger()
logger.setLevel(logging.ERROR)
