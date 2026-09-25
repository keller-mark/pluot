import os
import re
import sys
from pathlib import Path

# TODO: use matplotlib's FontManager?
# Reference: https://github.com/matplotlib/matplotlib/blob/main/lib/matplotlib/font_manager.py
# But this would introduce a matplotlib dependency.
# Is there a more lightweight alternative?

# Optional path overrides: font_family -> file path.
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
    2. System font detection.
    """
    if font_name in _FONT_OVERRIDES:
        return _FONT_OVERRIDES[font_name]
    if font_name not in _FONT_PATH_CACHE:
        _FONT_PATH_CACHE[font_name] = _find_system_font(font_name)
    return _FONT_PATH_CACHE[font_name]

def _key_to_font_family(key: str) -> str:
    """Extract the font family from a zarr-style key.

    Keys use the format "{family}/{style}/{weight}.ttf". The family is the
    first path segment. A leading "/" is stripped defensively if present.
    """
    # Strip any accidental leading slash then drop trailing .ttf/.otf.
    name = key.lstrip('/')
    if name.lower().endswith('.ttf') or name.lower().endswith('.otf'):
        name = name[:-4]
    # First segment is the family; remaining segments are style and weight.
    return name.split('/')[0]

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
    and key='{family}/{weight}/{style}.ttf'. The get_sync method is called by
    zarr_get_status to eagerly resolve synchronous results without waiting for
    an async call.
    """

    def get_sync(self, key: str) -> bytes:
        """Synchronously return font bytes, or raise if unavailable."""
        font_family = _key_to_font_family(key)
        path = _resolve_font_path(font_family)
        if path is None:
            raise FileNotFoundError(f"Font not found: {font_family}")
        with open(path, 'rb') as f:
            return f.read()

    async def get(self, key: str, prototype=None, byte_range=None) -> _BytesBuffer:
        """Async fallback for zarr_get cache-miss path."""
        return _BytesBuffer(self.get_sync(key))
