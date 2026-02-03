"""Meeting service detection module.

Detects meeting links from event descriptions using service-specific regex patterns.
Patterns ported from MeetingBar: https://github.com/leits/MeetingBar
"""

import re
import urllib.parse
from dataclasses import dataclass
from enum import Enum
from typing import Optional


class MeetingService(Enum):
    """Supported meeting services."""

    GOOGLE_MEET = "google_meet"
    ZOOM = "zoom"
    TEAMS = "teams"
    WEBEX = "webex"
    JITSI = "jitsi"
    SLACK = "slack"
    DISCORD = "discord"


@dataclass
class DetectedMeetingLink:
    """Detected meeting link with service information."""

    service: Optional[MeetingService]
    url: str


# Service-specific regex patterns (Tier 1)
# Patterns ported from MeetingBar's MeetingServices.swift
MEETING_PATTERNS: dict[MeetingService, re.Pattern] = {
    MeetingService.GOOGLE_MEET: re.compile(
        r"https?://meet\.google\.com/(_meet/)?[a-z-]+", re.IGNORECASE
    ),
    MeetingService.ZOOM: re.compile(
        r"https://(?:[a-zA-Z0-9-]+\.)?zoom(?:-x)?\.(?:us|com|com\.cn|de)/(?:my|[a-z]{1,2}|webinar)/[^\s]*",
        re.IGNORECASE,
    ),
    MeetingService.TEAMS: re.compile(
        r"https?://(gov\.)?teams\.microsoft\.(com|us)/l/meetup-join/[^\s]*",
        re.IGNORECASE,
    ),
    MeetingService.WEBEX: re.compile(
        r"https?://(?:[A-Za-z0-9-]+\.)?webex\.com(?:/wbx)?/(?:meet|join)/[^\s]*",
        re.IGNORECASE,
    ),
    MeetingService.JITSI: re.compile(r"https?://meet\.jit\.si/[^\s]*", re.IGNORECASE),
    MeetingService.SLACK: re.compile(
        r"https?://app\.slack\.com/huddle/[^\s]*", re.IGNORECASE
    ),
    MeetingService.DISCORD: re.compile(
        r"(?:http|https|discord)://(?:www\.)?(?:canary\.)?discord(?:app)?\.(?:[a-zA-Z]{2,})/channels/[^\s]*",
        re.IGNORECASE,
    ),
}
GENERIC_URL_PATTERN = re.compile(r"https?://\S+")


def cleanup_outlook_safelinks(url: str) -> str:
    """Clean up Outlook SafeLinks redirects.

    Args:
        url: URL that may contain Outlook SafeLinks redirect

    Returns:
        Cleaned URL with SafeLinks wrapper removed

    """
    # Remove Outlook SafeLinks wrapper
    if "safelinks.protection.outlook.com" in url:
        match = re.search(r"url=([^&]+)", url)
        if match:
            return urllib.parse.unquote(match.group(1))
    return url


def unwrap_outlook_safelinks(text: str) -> str:
    """Replace Outlook SafeLinks URLs in text with their decoded targets."""

    def replace(match: re.Match[str]) -> str:
        return cleanup_outlook_safelinks(match.group(0))

    return GENERIC_URL_PATTERN.sub(replace, text)


def detect_meeting_link(text: str) -> Optional[DetectedMeetingLink]:
    """Detect meeting link from text using service-specific patterns.

    Implements MeetingBar's detection algorithm with fallback:
    1. Cleanup Outlook SafeLinks per URL
    2. Early exit if no "://" in text
    3. Iterate through service patterns in priority order
    4. Fallback to generic URL pattern if no service match

    Args:
        text: Text to search for meeting links (description, location, etc.)

    Returns:
        DetectedMeetingLink if found, None otherwise

    """
    if not text:
        return None

    # Cleanup Outlook SafeLinks without dropping unrelated text.
    text = unwrap_outlook_safelinks(text)

    # Early exit if no URL-like pattern
    if "://" not in text:
        return None

    # Iterate through service patterns in priority order
    for service, pattern in MEETING_PATTERNS.items():
        match = pattern.search(text)
        if match:
            return DetectedMeetingLink(service=service, url=match.group(0))

    # Fallback to generic URL pattern for backward compatibility
    # This ensures existing behavior is preserved for non-service URLs
    match = GENERIC_URL_PATTERN.search(text)
    if match:
        return DetectedMeetingLink(service=None, url=match.group(0))

    return None
