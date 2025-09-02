import pytest

from pluot import render, render_to_array

@pytest.mark.asyncio
async def test_render_triangle():
    result = await render(width=100, height=100, plot_id="test", plot_type="Triangle", store_name="test")
    assert result is not None
    assert len(result) == 100 * 100 * 4  # RGBA for each pixel
    assert sum(result) == 5100000  # Expected sum for a triangle rendering

@pytest.mark.asyncio
async def test_render_to_array():
    arr = await render_to_array(width=100, height=200, plot_id="test", plot_type="Triangle", store_name="test")
    assert arr.shape == (200, 100, 4)
    assert arr.dtype == 'uint8'
    assert arr.sum() == 10200000