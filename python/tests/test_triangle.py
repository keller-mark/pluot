import pytest

from pluot import render_py

@pytest.mark.asyncio
async def test_render_triangle():
    result = await render_py(width=100, height=100, plotId="test", plotType="triangle", storeName="test")
    assert result is not None
    assert len(result) == 100 * 100 * 4  # RGBA for each pixel