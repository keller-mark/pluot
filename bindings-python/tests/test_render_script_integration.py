import os
from pathlib import Path

import pytest


def _normalize_svg(svg: str) -> str:
    """Trim each line and drop blanks, mirroring the Rust `check_svg_snapshot`
    normalization in crates/pluot/tests/test_utils/snapshot_utils.rs."""
    return "\n".join(line.strip() for line in svg.splitlines() if line.strip())


@pytest.mark.asyncio
async def test_render_script_integration_python():
    fixtures_dir = os.environ.get("PLUOT_RENDER_SCRIPT_FIXTURES_DIR")
    if not fixtures_dir:
        pytest.skip(
            "PLUOT_RENDER_SCRIPT_FIXTURES_DIR not set; run "
            "scripts/test_python_render_script_integration.sh instead of "
            "pytest directly to exercise this test"
        )
    fixtures_dir = Path(fixtures_dir)

    script = (fixtures_dir / "render_script.py").read_text()
    canonical_svg = (fixtures_dir / "canonical.svg").read_text()

    driver_src = script

    namespace = {}
    exec(compile(driver_src, "<render_script.py>", "exec"), namespace)
    img = await namespace["main"]()

    assert _normalize_svg(img) == _normalize_svg(canonical_svg)
