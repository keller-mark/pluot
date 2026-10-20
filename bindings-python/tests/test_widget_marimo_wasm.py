"""Renders PluotWasmWidget in a notebook exported with `marimo export html-wasm`.

Run via scripts/test_widget_marimo_wasm.sh, which installs the extra dependencies
(playwright, marimo) and a Chromium build. Pyodide, marimo's frontend, React, and
@pluot/react are all loaded from their CDNs, so this test requires network access.
"""

import io
import shutil
import subprocess
import sys
import threading
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import numpy as np
import pytest

playwright_sync_api = pytest.importorskip("playwright.sync_api")
pytest.importorskip("marimo")

REPO_ROOT = Path(__file__).resolve().parents[2]
NUM_POINTS = 100
# Pyodide startup plus micropip installs of numpy/zarr/marimo dominate this.
RENDER_TIMEOUT_MS = 240_000

NOTEBOOK_TEMPLATE = '''# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "marimo",
#     "zarr>=3",
#     "pluot_core @ {base_url}/public/{pluot_core_wheel}",
#     "pluot_widget @ {base_url}/public/{pluot_widget_wheel}",
# ]
# ///

import marimo

app = marimo.App()


@app.cell
def _():
    from fsspec.implementations.asyn_wrapper import AsyncFileSystemWrapper
    from fsspec.implementations.http_sync import HTTPFileSystem
    from zarr.storage import FsspecStore
    from pluot_widget import PluotWasmWidget

    fs = AsyncFileSystemWrapper(HTTPFileSystem(), asynchronous=True)
    store = FsspecStore(fs=fs, path="{base_url}/points.zarr", read_only=True)
    widget = PluotWasmWidget(
        store=store,
        store_name="points",
        width=400,
        height=400,
        format="{graphics_format}",
        plot_params=dict(
            layers=[
                dict(
                    layer_type="ZarrPointLayer",
                    layer_params=dict(
                        layer_id="points",
                        data_unit_mode_x="Data",
                        data_unit_mode_y="Data",
                        point_radius_unit_mode_x="Pixels",
                        point_radius_unit_mode_y="Pixels",
                        point_shape_mode="Circle",
                        x_key="/x",
                        y_key="/y",
                        color_key="/labels",
                        point_radius=5.0,
                        point_opacity=1.0,
                    ),
                ),
            ],
        ),
    )
    widget
    return


if __name__ == "__main__":
    app.run()
'''


class QuietHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


def build_wheel(package_dir: Path, out_dir: Path) -> str:
    subprocess.run(
        ["uv", "build", str(package_dir), "--wheel", "-o", str(out_dir)],
        check=True,
        capture_output=True,
    )
    [wheel] = out_dir.glob(f"{package_dir.name}-*.whl")
    return wheel.name


def write_points_zarr(path: Path) -> None:
    import zarr

    rng = np.random.default_rng(0)
    root = zarr.open_group(path, mode="w", zarr_format=3)
    # Uncompressed, since the WASM zarr reader lacks C-based codecs such as zstd.
    for name, data in [
        ("x", rng.normal(size=NUM_POINTS)),
        ("y", rng.normal(size=NUM_POINTS)),
        ("labels", rng.integers(0, 3, size=NUM_POINTS)),
    ]:
        root.create_array(name, data=data, compressors=None)


@pytest.fixture(scope="module")
def site(tmp_path_factory):
    if shutil.which("uv") is None:
        pytest.skip("uv is required to build the wheels")
    root = tmp_path_factory.mktemp("site")
    public = root / "public"
    public.mkdir()
    pluot_core_wheel = build_wheel(REPO_ROOT / "bindings-python" / "pluot_core", public)
    pluot_widget_wheel = build_wheel(REPO_ROOT / "bindings-python" / "pluot_widget", public)
    write_points_zarr(root / "points.zarr")

    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(root)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base_url = f"http://127.0.0.1:{server.server_address[1]}"
    yield dict(
        root=root,
        base_url=base_url,
        pluot_core_wheel=pluot_core_wheel,
        pluot_widget_wheel=pluot_widget_wheel,
    )
    server.shutdown()


def export_notebook(site: dict, graphics_format: str) -> str:
    notebook_path = site["root"] / f"notebook_{graphics_format}.py"
    notebook_path.write_text(NOTEBOOK_TEMPLATE.format(
        base_url=site["base_url"],
        pluot_core_wheel=site["pluot_core_wheel"],
        pluot_widget_wheel=site["pluot_widget_wheel"],
        graphics_format=graphics_format,
    ))
    out_dir = site["root"] / graphics_format
    subprocess.run(
        [sys.executable, "-m", "marimo", "export", "html-wasm", str(notebook_path), "-o", str(out_dir), "--mode", "run"],
        check=True,
        capture_output=True,
    )
    # marimo copies public/ next to the notebook into the export, but the
    # wheels are served from the site root, which keeps one copy for all exports.
    shutil.rmtree(out_dir / "public", ignore_errors=True)
    return f"{site['base_url']}/{graphics_format}/index.html"


@pytest.fixture(scope="module")
def browser():
    with playwright_sync_api.sync_playwright() as p:
        browser = p.chromium.launch(args=["--enable-unsafe-webgpu"])
        yield browser
        browser.close()


@pytest.fixture
def page(browser):
    page = browser.new_page()
    page_errors = []
    page.on("pageerror", lambda error: page_errors.append(error))
    yield page
    page.close()
    assert page_errors == []


def has_webgpu_adapter(page) -> bool:
    return page.evaluate("async () => !!(await navigator.gpu?.requestAdapter())")


def test_vector_widget_renders_points(site, page):
    page.goto(export_notebook(site, "Vector"))
    # Playwright locators pierce the open shadow root of marimo-anywidget, which also
    # keeps marimo's own icons out of the count.
    points = page.locator("marimo-anywidget svg circle")
    playwright_sync_api.expect(points).to_have_count(NUM_POINTS, timeout=RENDER_TIMEOUT_MS)


def test_raster_widget_renders_points(site, page):
    from PIL import Image

    url = export_notebook(site, "Raster")
    page.goto(url)
    if not has_webgpu_adapter(page):
        pytest.skip("WebGPU is unavailable, e.g., on CI runners without a GPU")

    canvas = page.locator("marimo-anywidget canvas")
    canvas.wait_for(state="visible", timeout=RENDER_TIMEOUT_MS)

    def count_colored_pixels() -> int:
        image = Image.open(io.BytesIO(canvas.screenshot())).convert("RGB")
        pixels = np.asarray(image).reshape(-1, 3).astype(int)
        return int(np.count_nonzero(pixels.max(axis=1) - pixels.min(axis=1) > 50))

    deadline_ms = RENDER_TIMEOUT_MS
    while count_colored_pixels() < NUM_POINTS and deadline_ms > 0:
        page.wait_for_timeout(1000)
        deadline_ms -= 1000
    assert count_colored_pixels() >= NUM_POINTS
