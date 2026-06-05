import os
import re
import sys
from pathlib import Path

# TODO: use matplotlib's FontManager?
# Reference: https://github.com/matplotlib/matplotlib/blob/main/lib/matplotlib/font_manager.py
# But this would introduce a matplotlib dependency.
# Is there a more lightweight alternative?

# Optional path overrides: font_name -> file path.
# Takes priority over system font detection.
_FONT_OVERRIDES: dict[str, str] = {}

def register_font(font_name: str, path: str) -> None:
    """Override the file path used for a named font. Takes priority over system detection."""
    _FONT_OVERRIDES[font_name] = path

def _normalize(name: str) -> str:
    return re.sub(r'[\s\-_]', '', name).lower()


# OS Font paths
try:
    _HOME = Path.home()
except Exception:  # Exceptions thrown by home() are not specified...
    _HOME = Path(os.devnull)  # Just an arbitrary path with no children.
MSFolders = \
    r'Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
MSFontDirectories = [
    r'SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts',
    r'SOFTWARE\Microsoft\Windows\CurrentVersion\Fonts']
MSUserFontDirectories = [
    str(_HOME / 'AppData/Local/Microsoft/Windows/Fonts'),
    str(_HOME / 'AppData/Roaming/Microsoft/Windows/Fonts'),
]
X11FontDirectories = [
    # an old standard installation point
    "/usr/X11R6/lib/X11/fonts/TTF/",
    "/usr/X11/lib/X11/fonts",
    # here is the new standard location for fonts
    "/usr/share/fonts/",
    # documented as a good place to install new fonts
    "/usr/local/share/fonts/",
    # common application, not really useful
    "/usr/lib/openoffice/share/fonts/truetype/",
    # user fonts
    str((Path(os.environ.get('XDG_DATA_HOME') or _HOME / ".local/share"))
        / "fonts"),
    str(_HOME / ".fonts"),
]
OSXFontDirectories = [
    "/Library/Fonts/",
    "/Network/Library/Fonts/",
    "/System/Library/Fonts/",
    # fonts installed via MacPorts
    "/opt/local/share/fonts",
    # user fonts
    str(_HOME / "Library/Fonts"),
]

# ---------------------------------------------------------------------------
# URW Core 35 fonts (permissive alternatives to the PDF Base-14 fonts).
#
# The TTF files live in the vendor/urw-core35-fonts git submodule.
# For proper PyPI distribution the fonts should be copied into the package
# (e.g. bindings-python/pluot/fonts/) and _URW_FONTS_DIR updated accordingly.
# ---------------------------------------------------------------------------

_URW_FONTS_DIR = Path(__file__).parent.parent.parent / 'vendor' / 'urw-core35-fonts'

# Maps the 14 PDF Base font names → TTF filename stems in _URW_FONTS_DIR.
# Only these names are recognised automatically. Any other font name (including
# direct URW names like "NimbusSans-Regular") must be registered explicitly via
# register_font() to be usable.
URW_FONT_MAP: dict[str, str] = {
    "Courier":               "NimbusMonoPS-Regular",
    "Courier-Bold":          "NimbusMonoPS-Bold",
    "Courier-Oblique":       "NimbusMonoPS-Italic",
    "Courier-BoldOblique":   "NimbusMonoPS-BoldItalic",
    "Helvetica":             "NimbusSans-Regular",
    "Helvetica-Bold":        "NimbusSans-Bold",
    "Helvetica-Oblique":     "NimbusSans-Oblique",
    "Helvetica-BoldOblique": "NimbusSans-BoldOblique",
    "Times-Roman":           "NimbusRoman-Regular",
    "Times-Bold":            "NimbusRoman-Bold",
    "Times-Italic":          "NimbusRoman-Italic",
    "Times-BoldItalic":      "NimbusRoman-BoldItalic",
    "Symbol":                "StandardSymbolsPS",
    "ZapfDingbats":          "D050000L",
}

def _system_font_dirs() -> list[Path]:
    if sys.platform == 'darwin':
        return [Path(d) for d in OSXFontDirectories]
    elif sys.platform == 'win32':
        return [Path(d) for d in MSUserFontDirectories]
    else:  # Linux / other
        return [Path(d) for d in X11FontDirectories]

def _find_system_font(font_name: str) -> str | None:
    norm_name = _normalize(font_name)
    for font_dir in _system_font_dirs():
        if not font_dir.exists():
            continue
        for path in font_dir.rglob('*'):
            if path.suffix.lower() not in ('.ttf', '.otf'):
                continue
            if _normalize(path.stem) == norm_name:
                return str(path)
    return None

# Cache results of _find_system_font to avoid redundant directory scans.
_FONT_PATH_CACHE: dict[str, str | None] = {}

def _resolve_font_path(font_name: str) -> str | None:
    """Return the file path for a font.

    Lookup order:
    1. Explicit override registered via register_font().
    2. URW Core 35 bundled fonts (by direct name or PDF Base-14 alias).
    3. System font detection.
    """
    if font_name in _FONT_OVERRIDES:
        return _FONT_OVERRIDES[font_name]
    urw_filename = URW_FONT_MAP.get(font_name)
    if urw_filename is not None:
        urw_path = _URW_FONTS_DIR / f'{urw_filename}.ttf'
        if urw_path.exists():
            return str(urw_path)
    if font_name not in _FONT_PATH_CACHE:
        _FONT_PATH_CACHE[font_name] = _find_system_font(font_name)
    return _FONT_PATH_CACHE[font_name]

def _key_to_font_name(key: str) -> str:
    """Extract the font name from a zarr-style key like 'Arial.ttf'."""
    name = key.lstrip('/')
    if name.lower().endswith('.ttf') or name.lower().endswith('.otf'):
        name = name[:-4]
    return name

class _BytesBuffer:
    """Minimal buffer wrapper compatible with zarr.py's `.to_bytes()` protocol."""
    def __init__(self, data: bytes):
        self._data = data
    def to_bytes(self) -> bytes:
        return self._data

class FontStore:
    """
    Store for fonts, registered as '__fonts__' in GLOBAL_STORES.
    Rust requests fonts via zarr_get_status / zarr_get with store_name='__fonts__'
    and key='{font_name}.ttf'. The get_sync method is called by zarr_get_status to
    eagerly resolve synchronous results without waiting for an async call.
    """

    def get_sync(self, key: str) -> bytes:
        """Synchronously return font bytes, or raise if unavailable."""
        font_name = _key_to_font_name(key)
        path = _resolve_font_path(font_name)
        if path is None:
            raise FileNotFoundError(f"Font not found: {font_name}")
        with open(path, 'rb') as f:
            return f.read()

    async def get(self, key: str, prototype=None, byte_range=None) -> _BytesBuffer:
        """Async fallback for zarr_get cache-miss path."""
        return _BytesBuffer(self.get_sync(key))
