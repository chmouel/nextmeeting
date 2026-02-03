"""Tests for gcalcli TSV parsing with conference and description fields."""

from nextmeeting.cli import REG_TSV, Meeting


class TestRegTSV:
    """Test the REG_TSV regex pattern."""

    def test_full_tsv_line_with_all_fields(self):
        """Test parsing a complete TSV line with all fields populated."""
        line = (
            "2026-02-03\t09:30\t2026-02-03\t10:00\t"
            "https://www.google.com/calendar/event?eid=abc123\t"
            "https://hangouts.google.com/call/xyz\t"
            "eventHangout\t"
            "https://meet.google.com/abc-defg-hij\t"
            "Daily Standup\t"
            "Join the meeting at https://youtube.com/live/12345"
        )
        match = REG_TSV.match(line)
        assert match is not None
        assert match["startdate"] == "2026-02-03"
        assert match["starthour"] == "09:30"
        assert match["enddate"] == "2026-02-03"
        assert match["endhour"] == "10:00"
        assert "abc123" in match["calendar_url"]
        assert match["hangout_link"] == "https://hangouts.google.com/call/xyz"
        assert match["conference_type"] == "eventHangout"
        assert match["conference_uri"] == "https://meet.google.com/abc-defg-hij"
        assert match["title"] == "Daily Standup"
        assert "youtube.com" in match["description"]

    def test_tsv_line_with_empty_optional_fields(self):
        """Test parsing a TSV line with empty hangout/conference fields."""
        line = (
            "2026-02-03\t14:00\t2026-02-03\t15:00\t"
            "https://www.google.com/calendar/event?eid=def456\t"
            "\t\t\t"  # Empty hangout_link, conference_type, conference_uri
            "Team Meeting\t"
            "Please join via https://zoom.us/j/123456789"
        )
        match = REG_TSV.match(line)
        assert match is not None
        assert match["hangout_link"] == ""
        assert match["conference_type"] == ""
        assert match["conference_uri"] == ""
        assert match["title"] == "Team Meeting"
        assert "zoom.us" in match["description"]

    def test_tsv_line_without_description(self):
        """Test parsing a TSV line with no description field."""
        line = (
            "2026-02-03\t16:00\t2026-02-03\t17:00\t"
            "https://www.google.com/calendar/event?eid=ghi789\t"
            "\t\t"
            "https://meet.google.com/xyz-abcd-efg\t"
            "Quick Sync"
        )
        match = REG_TSV.match(line)
        assert match is not None
        assert match["conference_uri"] == "https://meet.google.com/xyz-abcd-efg"
        assert match["title"] == "Quick Sync"
        assert match["description"] is None

    def test_all_day_event_empty_times(self):
        """Test parsing an all-day event with empty start/end times."""
        line = (
            "2026-02-03\t\t2026-02-04\t\t"
            "https://www.google.com/calendar/event?eid=allday\t"
            "\t\t\t"
            "Company Holiday"
        )
        match = REG_TSV.match(line)
        assert match is not None
        assert match["starthour"] is None or match["starthour"] == ""
        assert match["endhour"] is None or match["endhour"] == ""
        assert match["title"] == "Company Holiday"


class TestMeetingFromMatch:
    """Test Meeting.from_match() with new TSV format."""

    def test_uses_conference_uri_when_present(self):
        """Test that conference_uri is used as meet_url when available."""
        line = (
            "2026-02-03\t09:00\t2026-02-03\t10:00\t"
            "https://www.google.com/calendar/event?eid=test\t"
            "https://hangouts.google.com/call/old\t"
            "eventHangout\t"
            "https://meet.google.com/abc-defg-hij\t"
            "Test Meeting\t"
            "Description with https://zoom.us/j/12345"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        assert meeting.meet_url == "https://meet.google.com/abc-defg-hij"
        assert meeting.title == "Test Meeting"

    def test_uses_hangout_link_when_no_conference_uri(self):
        """Test that hangout_link is used when conference_uri is empty."""
        line = (
            "2026-02-03\t09:00\t2026-02-03\t10:00\t"
            "https://www.google.com/calendar/event?eid=test\t"
            "https://hangouts.google.com/call/legacy\t"
            "eventHangout\t"
            "\t"  # Empty conference_uri
            "Legacy Meeting\t"
            "Old hangout meeting"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        assert meeting.meet_url == "https://hangouts.google.com/call/legacy"

    def test_detects_youtube_link_in_description(self):
        """Test that YouTube links in description are detected."""
        line = (
            "2026-02-03\t10:00\t2026-02-03\t11:00\t"
            "https://www.google.com/calendar/event?eid=test\t"
            "\t\t\t"  # No hangout or conference
            "YouTube Live\t"
            "Watch at https://youtube.com/live/abc123"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        assert meeting.meet_url is not None
        assert "youtube.com" in meeting.meet_url

    def test_detects_zoom_link_in_description(self):
        """Test that Zoom links in description are detected."""
        line = (
            "2026-02-03\t11:00\t2026-02-03\t12:00\t"
            "https://www.google.com/calendar/event?eid=test\t"
            "\t\t\t"  # No hangout or conference
            "External Meeting\t"
            "Join us at https://zoom.us/j/987654321"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        assert meeting.meet_url is not None
        assert "zoom.us" in meeting.meet_url

    def test_detects_teams_link_in_description(self):
        """Test that Teams links in description are detected."""
        line = (
            "2026-02-03\t13:00\t2026-02-03\t14:00\t"
            "https://www.google.com/calendar/event?eid=test\t"
            "\t\t\t"
            "Vendor Call\t"
            "Click to join: https://teams.microsoft.com/l/meetup-join/abc123"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        assert meeting.meet_url is not None
        assert "teams.microsoft.com" in meeting.meet_url

    def test_no_meeting_link_when_none_present(self):
        """Test that meet_url is None when no link is present."""
        line = (
            "2026-02-03\t14:00\t2026-02-03\t15:00\t"
            "https://www.google.com/calendar/event?eid=test\t"
            "\t\t\t"
            "In-Person Meeting\t"
            "Conference Room A"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        assert meeting.meet_url is None

    def test_all_day_event_parsing(self):
        """Test parsing all-day events with empty times."""
        line = (
            "2026-02-03\t\t2026-02-04\t\t"
            "https://www.google.com/calendar/event?eid=allday\t"
            "\t\t\t"
            "Team Offsite"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        assert meeting.title == "Team Offsite"
        # All-day events should parse with 00:00 times
        assert meeting.start_time.hour == 0
        assert meeting.start_time.minute == 0

    def test_description_with_multiple_urls_picks_meeting_service(self):
        """Test that meeting service URLs are prioritized over generic URLs."""
        line = (
            "2026-02-03\t15:00\t2026-02-03\t16:00\t"
            "https://www.google.com/calendar/event?eid=test\t"
            "\t\t\t"
            "Review Meeting\t"
            "Agenda: https://docs.google.com/doc/123 Join: https://meet.google.com/xyz-abc-def"
        )
        match = REG_TSV.match(line)
        assert match is not None
        meeting = Meeting.from_match(match)
        # Should pick Google Meet link over Google Docs
        assert meeting.meet_url is not None
        assert "meet.google.com" in meeting.meet_url
