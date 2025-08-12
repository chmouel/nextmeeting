from nextmeeting.core import normalize_meet_url, extract_meeting_details


def test_zoom_join_to_j_path_and_passcode():
    u = "https://zoom.us/join?confno=123456789&pwd=abcDEF"
    norm = normalize_meet_url(u)
    assert norm.startswith("https://zoom.us/j/123456789")
    det = extract_meeting_details(u)
    assert det["service"] == "zoom"
    assert det["meeting_id"] == "123456789"
    assert det["passcode"] == "abcDEF"


def test_outlook_safelink_zoom():
    wrapped = (
        "https://nam12.safelinks.protection.outlook.com/ap/t-xyz/?url="
        + "https%3A%2F%2Fzoom.us%2Fjoin%3Fconfno%3D222%26pwd%3Dppp"
        + "&data=ignored"
    )
    norm = normalize_meet_url(wrapped)
    assert norm.startswith("https://zoom.us/j/222")
    det = extract_meeting_details(wrapped)
    assert det["meeting_id"] == "222"
