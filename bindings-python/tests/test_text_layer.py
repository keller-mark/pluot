"""
Tests for TextLayer font loading: PDF Base-14 names (via URW map) and custom TTF paths.
"""
import pytest
from pathlib import Path

from pluot import render_to_array
from pluot.font import register_font
from pluot.zarr import _RESULT_CACHE  # noqa: PLC2701 – cleared between tests

VENDOR_DIR = Path(__file__).parent.parent.parent / "vendor" / "urw-core35-fonts"

camera_view = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
]

def _text_layer_kwargs(font_family: str | None = None) -> dict:
    layer_params = dict(
        layer_id="font_test_layer",
        bounds=None,
        data_unit_mode_x="Data",
        data_unit_mode_y="Data",
        text_size=12.0,
        text_size_unit_mode="Pixels",
        text_align_mode="Middle",
        text_baseline_mode="Middle",
        text_rotation=None,
        font_family=font_family,
        font_weight="Normal",
        font_style="Normal",
        position_x={"dtype": "Float32", "values": [0.0, 1.0, 1.0, 0.0, 0.5]},
        position_y={"dtype": "Float32", "values": [0.0, 0.0, 1.0, 1.0, 0.5]},
        text_vec=["A", "B", "C", "D", "Hello"],
    )
    return dict(
        camera_view=camera_view,
        width=100,
        height=100,
        plot_id="font_test",
        plot_type="LayeredPlot",
        plot_params=dict(
            layers=[dict(layer_type="TextLayer", layer_params=layer_params)]
        ),
    )


@pytest.fixture(autouse=True)
def clear_font_cache():
    """Clear the zarr result cache between tests so each test resolves fresh."""
    _RESULT_CACHE.clear()
    yield
    _RESULT_CACHE.clear()


@pytest.mark.asyncio
async def test_text_layer_pdf_base14_font_helvetica():
    """PDF Base-14 name 'Helvetica' resolves to the bundled URW NimbusSans-Regular TTF."""
    arr = await render_to_array(**_text_layer_kwargs(font_family="Helvetica"))
    assert arr.shape == (100, 100, 4)
    assert arr.dtype == "uint8"
    assert arr.sum() > 0, "Rendered image should not be all black"


@pytest.mark.asyncio
async def test_text_layer_pdf_base14_font_courier():
    """PDF Base-14 name 'Courier' resolves to the bundled URW NimbusMonoPS-Regular TTF."""
    arr = await render_to_array(**_text_layer_kwargs(font_family="Courier"))
    assert arr.shape == (100, 100, 4)
    assert arr.dtype == "uint8"
    assert arr.sum() > 0


@pytest.mark.asyncio
async def test_text_layer_custom_ttf_font_file():
    """A custom TTF path registered via register_font() is loaded and renders correctly."""
    ttf_path = str(VENDOR_DIR / "NimbusRoman-Regular.ttf")
    assert Path(ttf_path).exists(), f"Vendor TTF not found: {ttf_path}"

    # Simulate a user pointing at a TTF inside node_modules (or any local path).
    register_font("CustomTestFont", ttf_path)
    try:
        arr = await render_to_array(**_text_layer_kwargs(font_family="CustomTestFont"))
        assert arr.shape == (100, 100, 4)
        assert arr.dtype == "uint8"
        assert arr.sum() > 0
    finally:
        # Clean up the override so other tests aren't affected.
        from pluot.font import _FONT_OVERRIDES
        _FONT_OVERRIDES.pop("CustomTestFont", None)
