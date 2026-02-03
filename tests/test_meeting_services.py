"""Tests for meeting service detection."""

import pytest

from nextmeeting.meeting_services import (
    MeetingService,
    cleanup_outlook_safelinks,
    detect_meeting_link,
)


class TestCleanupOutlookSafelinks:
    """Test Outlook SafeLinks cleanup."""

    def test_cleanup_safelinks_redirect(self):
        """Test cleaning Outlook SafeLinks redirect."""
        safelink = "https://na01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fzoom.us%2Fj%2F123456789&data=..."
        result = cleanup_outlook_safelinks(safelink)
        assert result == "https://zoom.us/j/123456789"

    def test_cleanup_no_safelinks(self):
        """Test that non-SafeLinks URLs are unchanged."""
        url = "https://zoom.us/j/123456789"
        result = cleanup_outlook_safelinks(url)
        assert result == url


class TestDetectMeetingLink:
    """Test meeting link detection."""

    def test_google_meet_basic(self):
        """Test Google Meet link detection."""
        text = "Join the meeting: https://meet.google.com/abc-defg-hij"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.GOOGLE_MEET
        assert result.url == "https://meet.google.com/abc-defg-hij"

    def test_google_meet_with_meet_prefix(self):
        """Test Google Meet link with _meet/ prefix."""
        text = "Meeting at https://meet.google.com/_meet/abc-defg-hij"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.GOOGLE_MEET
        assert "meet.google.com" in result.url

    def test_zoom_basic(self):
        """Test Zoom link detection."""
        text = "Join via https://zoom.us/j/123456789?pwd=abc123"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.ZOOM
        assert "zoom.us/j/123456789" in result.url

    def test_zoom_subdomain(self):
        """Test Zoom link with subdomain."""
        text = "https://company.zoom.us/my/personalroom"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.ZOOM
        assert "company.zoom.us" in result.url

    def test_teams_basic(self):
        """Test Microsoft Teams link detection."""
        text = (
            "Teams meeting: https://teams.microsoft.com/l/meetup-join/19%3ameeting_..."
        )
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.TEAMS
        assert "teams.microsoft.com" in result.url

    def test_teams_gov(self):
        """Test Microsoft Teams Gov link."""
        text = "https://gov.teams.microsoft.us/l/meetup-join/..."
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.TEAMS
        assert "gov.teams.microsoft.us" in result.url

    def test_webex_basic(self):
        """Test Webex link detection."""
        text = "Join Webex: https://company.webex.com/meet/username"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.WEBEX
        assert "webex.com/meet/" in result.url

    def test_webex_join(self):
        """Test Webex join link."""
        text = "https://company.webex.com/join/abc123"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.WEBEX

    def test_jitsi_basic(self):
        """Test Jitsi link detection."""
        text = "Join at https://meet.jit.si/RoomName123"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.JITSI
        assert result.url == "https://meet.jit.si/RoomName123"

    def test_slack_huddle(self):
        """Test Slack huddle link detection."""
        text = "Huddle: https://app.slack.com/huddle/T01234/C56789"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.SLACK
        assert "slack.com/huddle" in result.url

    def test_discord_basic(self):
        """Test Discord link detection."""
        text = "Voice chat: https://discord.com/channels/123456789/987654321"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.DISCORD
        assert "discord.com/channels" in result.url

    def test_discord_protocol(self):
        """Test Discord with discord:// protocol."""
        text = "discord://discord.com/channels/123/456"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.DISCORD

    def test_multiple_urls_returns_first_meeting_link(self):
        """Test that multiple URLs returns the first meeting link."""
        text = (
            "Check the doc: https://docs.google.com/document/d/123 "
            "Join meeting: https://meet.google.com/abc-defg-hij"
        )
        result = detect_meeting_link(text)
        # Should detect the Google Meet link, not the docs link
        assert result is not None
        assert result.service == MeetingService.GOOGLE_MEET

    def test_no_url_in_text(self):
        """Test that text without URLs returns None."""
        text = "This is just a regular meeting description with no links"
        result = detect_meeting_link(text)
        assert result is None

    def test_empty_text(self):
        """Test that empty text returns None."""
        result = detect_meeting_link("")
        assert result is None

    def test_none_text(self):
        """Test that None text returns None."""
        result = detect_meeting_link(None)
        assert result is None

    def test_non_meeting_url(self):
        """Test that non-meeting URLs are detected as generic."""
        text = "Visit our website: https://example.com"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service is None
        assert result.url == "https://example.com"

    def test_outlook_safelinks_unwrapping(self):
        """Test that Outlook SafeLinks are unwrapped before detection."""
        text = "Meeting: https://na01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fmeet.google.com%2Fabc-defg-hij&data=..."
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.GOOGLE_MEET
        assert "meet.google.com" in result.url
        assert "safelinks" not in result.url

    def test_safelinks_does_not_override_meeting_link(self):
        """Test that SafeLinks elsewhere do not override a meeting link."""
        text = (
            "Join: https://meet.google.com/abc-defg-hij "
            "Agenda: https://na01.safelinks.protection.outlook.com/?url="
            "https%3A%2F%2Fdocs.google.com%2Fdocument%2Fd%2F123&data=..."
        )
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.GOOGLE_MEET
        assert result.url == "https://meet.google.com/abc-defg-hij"

    def test_case_insensitive_matching(self):
        """Test that URL matching is case-insensitive."""
        text = "Join: HTTPS://MEET.GOOGLE.COM/abc-defg-hij"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.GOOGLE_MEET

    def test_generic_url_fallback(self):
        """Test that generic URLs are detected as fallback."""
        text = "Join via https://example.com/meeting/room123"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service is None
        assert result.url == "https://example.com/meeting/room123"

    def test_service_pattern_takes_precedence_over_generic(self):
        """Test that service-specific patterns take precedence over generic URL."""
        text = "Meeting: https://meet.google.com/abc-defg-hij and docs: https://example.com/doc"
        result = detect_meeting_link(text)
        assert result is not None
        assert result.service == MeetingService.GOOGLE_MEET
        assert "meet.google.com" in result.url
