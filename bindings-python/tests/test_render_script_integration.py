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

    # The generated script uses top-level `await` (its intended target is a
    # notebook cell); wrap it in an `async def` so it can run under plain
    # CPython, and have the wrapper return `img` so we can read it back.
    indented = "\n".join(f"    {line}" for line in script.splitlines())
    driver_src = f"async def _pluot_integration_main():\n{indented}\n    return img\n"

    namespace = {}
    exec(compile(driver_src, "<render_script.py>", "exec"), namespace)
    img = await namespace["_pluot_integration_main"]()

    assert _normalize_svg(img) == _normalize_svg(canonical_svg)
