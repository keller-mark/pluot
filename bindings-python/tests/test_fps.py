import csv
import time
from pathlib import Path

import numpy as np
import pytest

from pluot import render_to_array

CAMERA_VIEW = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
]

NUM_WARMUP = 3
NUM_FRAMES = 60   # renders per trial; FPS = NUM_FRAMES / elapsed
NUM_TRIALS = 100  # independent FPS measurements forming the distribution

CSV_PATH = Path(__file__).parent / "fps_results.csv"


def _point_layer_kwargs(n: int, width: int, height: int) -> dict:
    rng = np.random.default_rng(42)
    return dict(
        camera_view=CAMERA_VIEW,
        width=width,
        height=height,
        plot_id="fps_test",
        plot_type="LayeredPlot",
        store_name="fps_store",
        plot_params=dict(
            layers=[
                dict(
                    layer_type="PointLayer",
                    layer_params=dict(
                        layer_id="point_layer",
                        data_unit_mode_x="Data",
                        data_unit_mode_y="Data",
                        point_radius_unit_mode_x="Pixels",
                        point_radius_unit_mode_y="Pixels",
                        point_shape_mode="Circle",
                        point_radius=5.0,
                        bounds=None,
                        position_x=rng.random(n).tolist(),
                        position_y=rng.random(n).tolist(),
                        labels_vec=list(range(n)),
                    ),
                )
            ]
        ),
    )


def _zarr_point_layer_kwargs(n: int, width: int, height: int) -> dict:
    rng = np.random.default_rng(42)
    return dict(
        camera_view=CAMERA_VIEW,
        width=width,
        height=height,
        plot_id="fps_test_zarr",
        plot_type="LayeredPlot",
        plot_params=dict(
            layers=[
                dict(
                    layer_type="ZarrPointLayer",
                    layer_params=dict(
                        layer_id="zarr_point_layer",
                        data_unit_mode_x="Data",
                        data_unit_mode_y="Data",
                        point_radius_unit_mode_x="Pixels",
                        point_radius_unit_mode_y="Pixels",
                        point_shape_mode="Circle",
                        point_radius=3.0,
                        x_arr=rng.random(n).astype("<f8"),
                        y_arr=rng.random(n).astype("<f8"),
                        color_arr=np.zeros(n, dtype="<i8"),
                    ),
                )
            ]
        ),
    )


def _write_csv(layer_type: str, n_points: int, width: int, height: int, fps: np.ndarray) -> None:
    write_header = not CSV_PATH.exists()
    with CSV_PATH.open("a", newline="") as f:
        writer = csv.writer(f)
        if write_header:
            writer.writerow([
                "layer_type", "n_points", "width", "height", "trial", "fps",
            ])
        for i, v in enumerate(fps):
            writer.writerow([layer_type, n_points, width, height, i, round(v, 3)])


async def _measure_fps(kwargs: dict) -> np.ndarray:
    for _ in range(NUM_WARMUP):
        await render_to_array(**kwargs)
    samples = []
    for _ in range(NUM_TRIALS):
        t0 = time.perf_counter()
        for _ in range(NUM_FRAMES):
            await render_to_array(**kwargs)
        samples.append(NUM_FRAMES / (time.perf_counter() - t0))
    return np.array(samples)


@pytest.mark.asyncio
@pytest.mark.parametrize("n_points,width,height", [
    (10, 500, 500),
    (100, 500, 500),
    (1_000, 500, 500),
    (10_000, 500, 500),
    (10_000, 1920, 1080),
])
async def test_fps_point_layer(n_points, width, height):
    kwargs = _point_layer_kwargs(n_points, width, height)
    fps = await _measure_fps(kwargs)
    _write_csv("PointLayer", n_points, width, height, fps)
    assert fps.mean() > 0


@pytest.mark.asyncio
@pytest.mark.parametrize("n_points,width,height", [
    (1_000, 500, 500),
    (10_000, 500, 500),
    (100_000, 500, 500),
    (10_000, 1920, 1080),
])
async def test_fps_zarr_point_layer(n_points, width, height):
    kwargs = _zarr_point_layer_kwargs(n_points, width, height)
    fps = await _measure_fps(kwargs)
    _write_csv("ZarrPointLayer", n_points, width, height, fps)
    assert fps.mean() > 0
