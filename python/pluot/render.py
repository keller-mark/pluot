from PIL import Image
import numpy as np
from ._internal import render_py


async def render(**kwargs):
    """Render to raw bytes."""
    # We wrap the internal function here to be able to provide types, docstrings, etc.
    result = await render_py(**kwargs)
    return result

async def render_to_array(**kwargs):
    """Render to a NumPy array, with shape (height, width, RGBA)."""
    width = kwargs["width"]
    height = kwargs["height"]
    result = await render(**kwargs)
    arr = np.frombuffer(result, dtype=np.dtype('uint8')).reshape((height, width, 4))
    return arr

async def render_to_image(**kwargs):
    arr = await render_to_array(**kwargs)
    img = Image.fromarray(arr)
    return img