"""nextmeeting: socket-based client/server utilities."""

from importlib.metadata import version as _pkg_version

__all__ = ["__version__"]

try:
    __version__ = _pkg_version("nextmeeting")
except Exception:  # pragma: no cover - during editable installs
    __version__ = "0.0.0"
