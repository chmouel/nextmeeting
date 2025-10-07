import tempfile
import os
import sys
from pathlib import Path
from unittest.mock import patch

from nextmeeting.cli import _load_config, parse_args


def test_load_config_normalizes_hyphens_to_underscores():
    """Test that configuration keys with hyphens are normalized to underscores."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
        f.write("""[nextmeeting]
caldav-url = "https://example.com/calendar"
caldav-username = "user"
caldav-password = "pass"
max-title-length = 30
today-only = true
""")
        config_path = Path(f.name)

    try:
        config = _load_config(config_path)

        # All keys should be normalized to underscores
        assert config["caldav_url"] == "https://example.com/calendar"
        assert config["caldav_username"] == "user"
        assert config["caldav_password"] == "pass"
        assert config["max_title_length"] == 30
        assert config["today_only"] is True

        # Hyphens should not exist in the keys
        assert "caldav-url" not in config
        assert "caldav-username" not in config
        assert "caldav-password" not in config
        assert "max-title-length" not in config
        assert "today-only" not in config
    finally:
        os.unlink(config_path)


def test_load_config_works_with_both_formats():
    """Test that both hyphen and underscore formats work in config files."""
    # Test with hyphens (README format)
    with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
        f.write("""[nextmeeting]
caldav-url = "https://example.com/hyphens"
caldav-username = "user-hyphens"
""")
        config_path_hyphens = Path(f.name)

    # Test with underscores (internal format)
    with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
        f.write("""[nextmeeting]
caldav_url = "https://example.com/underscores"
caldav_username = "user_underscores"
""")
        config_path_underscores = Path(f.name)

    try:
        # Both should work and produce the same key format
        config_hyphens = _load_config(config_path_hyphens)
        config_underscores = _load_config(config_path_underscores)

        # Both should have normalized underscore keys
        assert "caldav_url" in config_hyphens
        assert "caldav_username" in config_hyphens
        assert "caldav_url" in config_underscores
        assert "caldav_username" in config_underscores

        # Values should be preserved
        assert config_hyphens["caldav_url"] == "https://example.com/hyphens"
        assert config_hyphens["caldav_username"] == "user-hyphens"
        assert config_underscores["caldav_url"] == "https://example.com/underscores"
        assert config_underscores["caldav_username"] == "user_underscores"

    finally:
        os.unlink(config_path_hyphens)
        os.unlink(config_path_underscores)


def test_parse_args_accepts_config_with_hyphens():
    """Test that parse_args properly handles config files with hyphenated keys."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
        f.write("""[nextmeeting]
caldav-url = "https://example.com/config-test"
caldav-username = "config-user"
max-title-length = 25
""")
        config_path = f.name

    # Clear environment variables that might interfere
    with patch.dict(os.environ, {}, clear=True):
        original_argv = sys.argv
        try:
            sys.argv = ["nextmeeting", "--config", config_path]
            args = parse_args()

            # The arguments should be accessible with underscores
            assert getattr(args, "caldav_url") == "https://example.com/config-test"
            assert getattr(args, "caldav_username") == "config-user"
            assert getattr(args, "max_title_length") == 25

        finally:
            sys.argv = original_argv
            os.unlink(config_path)
