from __future__ import annotations
from typing import Literal, Optional
from dataclasses import dataclass, field
import numpy as np

# TODO: auto-generate these types from the Rust side: https://github.com/keller-mark/pluot/issues/133
AspectRatioMode = Literal["Ignore", "Contain", "Cover"]
AspectRatioAlignmentMode = Literal["Center", "Start", "End"]


@dataclass
class Margins:
    margin_top: float = 0.0
    margin_right: float = 0.0
    margin_bottom: float = 0.0
    margin_left: float = 0.0


@dataclass
class ViewportParams:
    width: float
    height: float
    aspect_ratio_mode: AspectRatioMode
    aspect_ratio_alignment_mode: AspectRatioAlignmentMode
    margins: Optional[Margins] = field(default=None)


@dataclass
class Bounds:
    # Each value is optional.
    # When an entire dimension is omitted (X or Y),
    # use the current camera settings for that dimension.
    # When a single value is omitted (e.g., x_min), ensure the resulting camera matrix keeps this boundary unchanged.
    x_min: Optional[float] = None
    x_max: Optional[float] = None
    y_min: Optional[float] = None
    y_max: Optional[float] = None


def _get_scales_and_align_translations(viewport_params: ViewportParams):
    margins = viewport_params.margins
    margin_top = margins.margin_top if margins else 0.0
    margin_right = margins.margin_right if margins else 0.0
    margin_bottom = margins.margin_bottom if margins else 0.0
    margin_left = margins.margin_left if margins else 0.0

    layer_w = viewport_params.width - margin_left - margin_right
    layer_h = viewport_params.height - margin_top - margin_bottom
    layer_aspect_ratio = layer_w / layer_h

    x_scale = 1.0
    y_scale = 1.0
    if viewport_params.aspect_ratio_mode == "Contain":
        if layer_aspect_ratio > 1.0:
            x_scale = layer_aspect_ratio
        elif layer_aspect_ratio < 1.0:
            y_scale = 1.0 / layer_aspect_ratio
    elif viewport_params.aspect_ratio_mode == "Cover":
        if layer_aspect_ratio > 1.0:
            y_scale = 1.0 / layer_aspect_ratio
        elif layer_aspect_ratio < 1.0:
            x_scale = layer_aspect_ratio

    x_align_translation = 0.0
    y_align_translation = 0.0
    if viewport_params.aspect_ratio_alignment_mode == "Start":
        x_align_translation = x_scale - 1.0
        y_align_translation = y_scale - 1.0
    elif viewport_params.aspect_ratio_alignment_mode == "End":
        x_align_translation = 1.0 - x_scale
        y_align_translation = 1.0 - y_scale

    return x_scale, y_scale, x_align_translation, y_align_translation


def get_bounds(camera_matrix: np.ndarray, viewport_params: ViewportParams) -> Bounds:
    """Calculate the visible data range based on camera view and viewport parameters."""
    zoom_x = camera_matrix[0]
    zoom_y = camera_matrix[5]
    translate_x = camera_matrix[12]
    translate_y = camera_matrix[13]

    x_scale, y_scale, x_align_translation, y_align_translation = _get_scales_and_align_translations(viewport_params)

    x_adj = x_scale - 1.0
    y_adj = y_scale - 1.0

    x_min = ((-translate_x - 1.0 - x_adj + x_align_translation) / zoom_x + 1.0) / 2.0
    x_max = ((-translate_x + 1.0 + x_adj + x_align_translation) / zoom_x + 1.0) / 2.0
    y_min = ((-translate_y - 1.0 - y_adj + y_align_translation) / zoom_y + 1.0) / 2.0
    y_max = ((-translate_y + 1.0 + y_adj + y_align_translation) / zoom_y + 1.0) / 2.0

    return Bounds(x_min=x_min, x_max=x_max, y_min=y_min, y_max=y_max)


def get_camera_matrix_from_bounds(bounds: Bounds, prev_camera_matrix: np.ndarray, viewport_params: ViewportParams) -> np.ndarray:
    """Given data bounds, compute the corresponding camera matrix.
    Missing bound values are filled in from prev_camera_matrix.
    """
    current_bounds = get_bounds(prev_camera_matrix, viewport_params)
    x_min = bounds.x_min if bounds.x_min is not None else current_bounds.x_min
    x_max = bounds.x_max if bounds.x_max is not None else current_bounds.x_max
    y_min = bounds.y_min if bounds.y_min is not None else current_bounds.y_min
    y_max = bounds.y_max if bounds.y_max is not None else current_bounds.y_max

    x_scale, y_scale, x_align_translation, y_align_translation = _get_scales_and_align_translations(viewport_params)

    x_adj = x_scale - 1.0
    y_adj = y_scale - 1.0

    x_range = x_max - x_min
    y_range = y_max - y_min

    zoom_x = (1.0 + x_adj) / x_range
    zoom_y = (1.0 + y_adj) / y_range

    # When aspect ratio is ignored, zoom each axis independently.
    # Otherwise take the minimum so all requested data fits within the viewport.
    if viewport_params.aspect_ratio_mode != "Ignore":
        zoom_x = zoom_y = min(zoom_x, zoom_y)

    # Invert the get_bounds translation equations:
    #   min + max = (-translate + align) / zoom + 1.0
    # So: translate = align - zoom * ((min + max) - 1.0)
    translate_x = x_align_translation - zoom_x * ((x_min + x_max) - 1.0)
    translate_y = y_align_translation - zoom_y * ((y_min + y_max) - 1.0)

    return np.array([
        zoom_x, 0.0,   0.0, 0.0,
        0.0,   zoom_y, 0.0, 0.0,
        0.0,   0.0,    1.0, 0.0,
        translate_x, translate_y, 0.0, 1.0,
    ], dtype=np.float32)
