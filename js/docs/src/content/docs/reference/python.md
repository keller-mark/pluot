---
title: Usage from Python
description: How to render plots from Python.
sidebar:
    # Set a custom order for the link (lower numbers are displayed higher up)
    order: 10
---

Install the `pluot` Python package from [PyPI](https://pypi.org/project/pluot/).

```sh frame="none"
pip install pluot
```

## Static plotting

Use the `render_to_array` and `render_to_image` functions to render static plots in raster format.
The former returns a Numpy array, while the latter returns a `PIL.Image` object.

When used in a Jupyter notebook, the returned `PIL.Image` object will be displayed as an image output of the notebook cell.

```py
from pluot import render_to_image
import numpy as np

camera_view = [
    0.15, 0.0, 0.0, 0.0,
    0.0, 0.15, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
]

x_arr = ((np.random.rand(500) - 0.5) * 10.0).astype('<f8')
y_arr = ((np.random.rand(500) - 0.5) * 10.0).astype('<f8')
color_arr = np.array(
  [0, 1, 2, 3, 4] * 100
).astype('<i8')

await render_to_image(
    camera_view=camera_view,
    width=700,
    height=800,
    plot_id="test",
    plot_type="Scatterplot",
    plot_params=dict(
        x_arr=x_arr,
        y_arr=y_arr,
        color_arr=color_arr,
        point_radius=10.0
    ),
    margin_left=100,
    margin_bottom=100
)
```

## Async runtime

Since the plotting function is `async`, it must be called from a Python async runtime.
Code executed in Jupyter notebooks is already running in an async runtime, so `await render_to_image` will "just work".

However, in other contexts, be careful to ensure `render_to_image` is `await`-ed within an async runtime.
For instance, to use within a REPL, run `python -m asyncio`.


## Interactive notebook widget

TODO

