//! Link detection and normalization for meeting URLs.
//!
//! This module provides functionality to:
//! - Extract meeting URLs from text (event descriptions, locations)
//! - Unwrap Microsoft Outlook SafeLinks
//! - Detect and classify video conferencing services (Zoom, Meet, Teams, Jitsi)
//! - Normalize URLs and extract meeting IDs and passcodes
//!
//! # Example
//!
//! ```
//! use nextmeeting_core::links::{LinkDetector, extract_links_from_text};
//!
//! let text = "Join us at https://zoom.us/j/123456789?pwd=abc123";
//! let links = extract_links_from_text(text);
//! assert_eq!(links.len(), 1);
//! assert!(links[0].kind.is_video_conference());
//! ```

use std::sync::LazyLock;

use regex::Regex;
use url::Url;

use crate::event::{EventLink, LinkKind};

/// Regex for extracting URLs from text.
static URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<>"'\)\]]+"#).expect("Invalid URL regex"));

/// Regex for detecting Microsoft Outlook SafeLinks.
///
/// SafeLinks wrap the original URL in a redirect through `safelinks.protection.outlook.com`.
/// The original URL is encoded in the `url` query parameter.
static SAFELINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://[^/]*safelinks\.protection\.outlook\.com/?\?[^?]*url=([^&]+)")
        .expect("Invalid SafeLink regex")
});

/// Regex for detecting Zoom meeting URLs.
static ZOOM_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://([^/]*\.)?zoom\.us/").expect("Invalid Zoom regex"));

/// Regex for detecting Zoom for Government meeting URLs.
static ZOOMGOV_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://([^/]*\.)?zoomgov\.com/").expect("Invalid ZoomGov regex")
});

/// Regex for detecting Google Meet URLs.
static MEET_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://meet\.google\.com/").expect("Invalid Meet regex"));

/// Regex for detecting Microsoft Teams meeting URLs.
static TEAMS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://teams\.(microsoft\.com|live\.com)/").expect("Invalid Teams regex")
});

/// Regex for detecting Jitsi Meet URLs (official and self-hosted).
static JITSI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://meet\.jit\.si/").expect("Invalid Jitsi regex"));

/// Link detector and normalizer.
///
/// This struct provides methods to detect, classify, and normalize meeting URLs
/// from various video conferencing services.
#[derive(Debug, Default)]
pub struct LinkDetector;

impl LinkDetector {
    /// Creates a new link detector.
    pub fn new() -> Self {
        Self
    }

    /// Extracts all URLs from the given text.
    ///
    /// Returns raw URLs without normalization or classification.
    pub fn extract_urls(&self, text: &str) -> Vec<String> {
        URL_REGEX
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Detects and normalizes a single URL into an EventLink.
    ///
    /// This method:
    /// 1. Unwraps Outlook SafeLinks if present
    /// 2. Detects the video conferencing service
    /// 3. Normalizes the URL (removes tracking params, standardizes format)
    /// 4. Extracts meeting ID and passcode where applicable
    pub fn detect(&self, url: &str) -> EventLink {
        // First, unwrap SafeLinks
        let unwrapped = unwrap_safelink(url);

        // Detect the service and normalize
        if ZOOMGOV_REGEX.is_match(&unwrapped) {
            normalize_zoom(&unwrapped, true)
        } else if ZOOM_REGEX.is_match(&unwrapped) {
            normalize_zoom(&unwrapped, false)
        } else if MEET_REGEX.is_match(&unwrapped) {
            normalize_meet(&unwrapped)
        } else if TEAMS_REGEX.is_match(&unwrapped) {
            normalize_teams(&unwrapped)
        } else if JITSI_REGEX.is_match(&unwrapped) {
            normalize_jitsi(&unwrapped)
        } else {
            EventLink::new(LinkKind::Other, unwrapped)
        }
    }

    /// Extracts and processes all meeting links from text.
    ///
    /// This is a convenience method that combines URL extraction and detection.
    /// Duplicate URLs (by normalized form) are removed.
    pub fn extract_from_text(&self, text: &str) -> Vec<EventLink> {
        let urls = self.extract_urls(text);
        let mut seen = std::collections::HashSet::new();
        let mut links = Vec::new();

        for url in urls {
            let link = self.detect(&url);
            // Deduplicate by URL
            if seen.insert(link.url.clone()) {
                links.push(link);
            }
        }

        // Sort video conference links first
        links.sort_by_key(|l| !l.kind.is_video_conference());
        links
    }
}

/// Convenience function to extract links from text.
///
/// See [`LinkDetector::extract_from_text`] for details.
pub fn extract_links_from_text(text: &str) -> Vec<EventLink> {
    LinkDetector::new().extract_from_text(text)
}

/// Convenience function to detect and normalize a single URL.
///
/// See [`LinkDetector::detect`] for details.
pub fn detect_link(url: &str) -> EventLink {
    LinkDetector::new().detect(url)
}

/// Unwraps a Microsoft Outlook SafeLink to get the original URL.
///
/// SafeLinks are used by Microsoft 365 to protect users from malicious links.
/// They redirect through `safelinks.protection.outlook.com` with the original
/// URL encoded in the `url` query parameter.
///
/// If the URL is not a SafeLink, it is returned unchanged.
fn unwrap_safelink(url: &str) -> String {
    if let Some(caps) = SAFELINK_REGEX.captures(url) {
        if let Some(encoded) = caps.get(1) {
            // URL-decode the original link
            if let Ok(decoded) = urlencoding::decode(encoded.as_str()) {
                return decoded.into_owned();
            }
        }
    }
    url.to_string()
}

/// Normalizes a Zoom URL and extracts meeting credentials.
///
/// Handles various Zoom URL formats:
/// - `zoom.us/j/<meeting_id>` (standard join link)
/// - `zoom.us/join?confno=<meeting_id>` (alternative join format)
/// - `zoom.us/my/<personal_id>` (personal meeting room)
///
/// Extracts passcode from `pwd` or `passcode` query parameters.
fn normalize_zoom(url: &str, is_gov: bool) -> EventLink {
    let kind = if is_gov {
        LinkKind::ZoomGov
    } else {
        LinkKind::Zoom
    };

    let Ok(parsed) = Url::parse(url) else {
        return EventLink::new(kind, url);
    };

    let mut meeting_id: Option<String> = None;
    let mut passcode: Option<String> = None;

    // Extract passcode from query parameters
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "pwd" | "passcode" => passcode = Some(value.into_owned()),
            "confno" => meeting_id = Some(value.into_owned()),
            _ => {}
        }
    }

    // Extract meeting ID from path
    let path = parsed.path();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.len() >= 2 {
        match segments[0] {
            "j" | "my" | "w" | "wc" => {
                // /j/<id>, /my/<id>, /w/<id>, /wc/<id>
                if meeting_id.is_none() {
                    meeting_id = Some(segments[1].to_string());
                }
            }
            "join" => {
                // /join?confno=<id> - meeting ID already extracted from query
            }
            _ => {}
        }
    }

    // Build normalized URL
    let host = if is_gov { "zoomgov.com" } else { "zoom.us" };
    let normalized = if let Some(ref id) = meeting_id {
        let mut new_url = format!("https://{}/j/{}", host, id);
        if let Some(ref pwd) = passcode {
            new_url.push_str("?pwd=");
            new_url.push_str(pwd);
        }
        new_url
    } else {
        // Keep original if we couldn't parse the meeting ID
        url.to_string()
    };

    EventLink::with_credentials(kind, normalized, meeting_id, passcode)
}

/// Normalizes a Google Meet URL and extracts the meeting code.
///
/// Meet URLs have the format: `meet.google.com/<meeting-code>`
/// where the meeting code is typically like `abc-defg-hij`.
fn normalize_meet(url: &str) -> EventLink {
    let Ok(parsed) = Url::parse(url) else {
        return EventLink::new(LinkKind::GoogleMeet, url);
    };

    // Extract meeting code from path
    let path = parsed.path();
    let meeting_id = path
        .split('/')
        .filter(|s| !s.is_empty())
        .next()
        .map(|s| s.to_string());

    // Build clean URL without query parameters
    let normalized = if let Some(ref id) = meeting_id {
        format!("https://meet.google.com/{}", id)
    } else {
        url.to_string()
    };

    EventLink::with_credentials(LinkKind::GoogleMeet, normalized, meeting_id, None)
}

/// Normalizes a Microsoft Teams URL.
///
/// Teams URLs are long and contain signed tokens, so we keep them mostly as-is.
/// We only do basic cleanup like removing trailing whitespace.
fn normalize_teams(url: &str) -> EventLink {
    // Teams links are long and signed; keep as-is
    EventLink::new(LinkKind::Teams, url.trim())
}

/// Normalizes a Jitsi Meet URL and extracts the room name.
///
/// Jitsi URLs have the format: `meet.jit.si/<room-name>`
fn normalize_jitsi(url: &str) -> EventLink {
    let Ok(parsed) = Url::parse(url) else {
        return EventLink::new(LinkKind::Jitsi, url);
    };

    // Extract room name from path
    let path = parsed.path();
    let meeting_id = path
        .split('/')
        .filter(|s| !s.is_empty())
        .next()
        .map(|s| s.to_string());

    // Build clean URL without query parameters
    let normalized = if let Some(ref id) = meeting_id {
        format!("https://meet.jit.si/{}", id)
    } else {
        url.to_string()
    };

    EventLink::with_credentials(LinkKind::Jitsi, normalized, meeting_id, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod safelinks {
        use super::*;

        #[test]
        fn unwraps_safelink() {
            let safelink = "https://nam01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fzoom.us%2Fj%2F123456789&data=abc123";
            let result = unwrap_safelink(safelink);
            assert_eq!(result, "https://zoom.us/j/123456789");
        }

        #[test]
        fn unwraps_complex_safelink() {
            let safelink = "https://eur01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fmeet.google.com%2Fabc-defg-hij%3Fauthuser%3D0&data=xyz&sdata=qrs";
            let result = unwrap_safelink(safelink);
            assert_eq!(result, "https://meet.google.com/abc-defg-hij?authuser=0");
        }

        #[test]
        fn returns_non_safelink_unchanged() {
            let url = "https://zoom.us/j/123456789";
            let result = unwrap_safelink(url);
            assert_eq!(result, url);
        }
    }

    mod zoom {
        use super::*;

        #[test]
        fn normalizes_standard_zoom_link() {
            let link = detect_link("https://zoom.us/j/123456789");
            assert_eq!(link.kind, LinkKind::Zoom);
            assert_eq!(link.url, "https://zoom.us/j/123456789");
            assert_eq!(link.meeting_id, Some("123456789".to_string()));
            assert_eq!(link.passcode, None);
        }

        #[test]
        fn normalizes_zoom_with_passcode() {
            let link = detect_link("https://zoom.us/j/123456789?pwd=abc123XYZ");
            assert_eq!(link.kind, LinkKind::Zoom);
            assert_eq!(link.url, "https://zoom.us/j/123456789?pwd=abc123XYZ");
            assert_eq!(link.meeting_id, Some("123456789".to_string()));
            assert_eq!(link.passcode, Some("abc123XYZ".to_string()));
        }

        #[test]
        fn normalizes_zoom_join_format() {
            let link = detect_link("https://zoom.us/join?confno=987654321&pwd=secret");
            assert_eq!(link.kind, LinkKind::Zoom);
            assert_eq!(link.meeting_id, Some("987654321".to_string()));
            assert_eq!(link.passcode, Some("secret".to_string()));
        }

        #[test]
        fn normalizes_zoom_personal_room() {
            let link = detect_link("https://zoom.us/my/johndoe");
            assert_eq!(link.kind, LinkKind::Zoom);
            assert_eq!(link.meeting_id, Some("johndoe".to_string()));
        }

        #[test]
        fn normalizes_zoom_with_subdomain() {
            let link = detect_link("https://company.zoom.us/j/123456789");
            assert_eq!(link.kind, LinkKind::Zoom);
            assert_eq!(link.meeting_id, Some("123456789".to_string()));
        }

        #[test]
        fn handles_zoomgov() {
            let link = detect_link("https://example.zoomgov.com/j/123456789");
            assert_eq!(link.kind, LinkKind::ZoomGov);
            assert!(link.url.contains("zoomgov.com"));
        }

        #[test]
        fn strips_tracking_params() {
            let link =
                detect_link("https://zoom.us/j/123?pwd=abc&utm_source=email&utm_medium=calendar");
            assert_eq!(link.url, "https://zoom.us/j/123?pwd=abc");
            assert_eq!(link.passcode, Some("abc".to_string()));
        }
    }

    mod google_meet {
        use super::*;

        #[test]
        fn normalizes_meet_link() {
            let link = detect_link("https://meet.google.com/abc-defg-hij");
            assert_eq!(link.kind, LinkKind::GoogleMeet);
            assert_eq!(link.url, "https://meet.google.com/abc-defg-hij");
            assert_eq!(link.meeting_id, Some("abc-defg-hij".to_string()));
        }

        #[test]
        fn strips_meet_query_params() {
            let link = detect_link("https://meet.google.com/abc-defg-hij?authuser=0&hs=179");
            assert_eq!(link.url, "https://meet.google.com/abc-defg-hij");
        }

        #[test]
        fn handles_meet_with_trailing_slash() {
            let link = detect_link("https://meet.google.com/xyz-uvwx-rst/");
            assert_eq!(link.meeting_id, Some("xyz-uvwx-rst".to_string()));
        }
    }

    mod teams {
        use super::*;

        #[test]
        fn keeps_teams_link_intact() {
            let url = "https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc123@thread.v2/0?context=%7b%22Tid%22%3a%22xyz%22%7d";
            let link = detect_link(url);
            assert_eq!(link.kind, LinkKind::Teams);
            assert_eq!(link.url, url);
        }

        #[test]
        fn handles_teams_live() {
            let url = "https://teams.live.com/meet/abc123";
            let link = detect_link(url);
            assert_eq!(link.kind, LinkKind::Teams);
        }
    }

    mod jitsi {
        use super::*;

        #[test]
        fn normalizes_jitsi_link() {
            let link = detect_link("https://meet.jit.si/MyMeetingRoom");
            assert_eq!(link.kind, LinkKind::Jitsi);
            assert_eq!(link.url, "https://meet.jit.si/MyMeetingRoom");
            assert_eq!(link.meeting_id, Some("MyMeetingRoom".to_string()));
        }

        #[test]
        fn strips_jitsi_query_params() {
            let link = detect_link("https://meet.jit.si/TestRoom?config.startWithAudioMuted=true");
            assert_eq!(link.url, "https://meet.jit.si/TestRoom");
        }
    }

    mod extract_from_text {
        use super::*;

        #[test]
        fn extracts_single_link() {
            let text = "Join the meeting at https://zoom.us/j/123456789";
            let links = extract_links_from_text(text);
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].kind, LinkKind::Zoom);
        }

        #[test]
        fn extracts_multiple_links() {
            let text = r#"
                Primary: https://meet.google.com/abc-defg-hij
                Backup: https://zoom.us/j/999888777
                Docs: https://docs.google.com/document/d/abc123
            "#;
            let links = extract_links_from_text(text);
            assert_eq!(links.len(), 3);
            // Video conference links should be first
            assert!(links[0].kind.is_video_conference());
            assert!(links[1].kind.is_video_conference());
            assert!(!links[2].kind.is_video_conference());
        }

        #[test]
        fn deduplicates_urls() {
            let text = r#"
                Click here: https://zoom.us/j/123
                Or here: https://zoom.us/j/123
            "#;
            let links = extract_links_from_text(text);
            assert_eq!(links.len(), 1);
        }

        #[test]
        fn unwraps_safelinks_in_text() {
            let text = "Join: https://nam01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fzoom.us%2Fj%2F123456789";
            let links = extract_links_from_text(text);
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].kind, LinkKind::Zoom);
            assert!(links[0].url.contains("zoom.us"));
        }

        #[test]
        fn handles_empty_text() {
            let links = extract_links_from_text("");
            assert!(links.is_empty());
        }

        #[test]
        fn handles_text_without_urls() {
            let links = extract_links_from_text("This is just plain text with no URLs.");
            assert!(links.is_empty());
        }

        #[test]
        fn extracts_from_html() {
            let text = r#"<a href="https://meet.google.com/abc-defg-hij">Join Meeting</a>"#;
            let links = extract_links_from_text(text);
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].kind, LinkKind::GoogleMeet);
        }
    }

    mod link_detector {
        use super::*;

        #[test]
        fn detector_is_reusable() {
            let detector = LinkDetector::new();

            let link1 = detector.detect("https://zoom.us/j/111");
            let link2 = detector.detect("https://meet.google.com/abc");

            assert_eq!(link1.kind, LinkKind::Zoom);
            assert_eq!(link2.kind, LinkKind::GoogleMeet);
        }

        #[test]
        fn extract_urls_finds_all() {
            let detector = LinkDetector::new();
            let text = "Check https://example.com and also http://test.org/path";
            let urls = detector.extract_urls(text);
            assert_eq!(urls.len(), 2);
        }
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn handles_malformed_url() {
            let link = detect_link("not-a-valid-url");
            assert_eq!(link.kind, LinkKind::Other);
            assert_eq!(link.url, "not-a-valid-url");
        }

        #[test]
        fn handles_url_with_unicode() {
            let link = detect_link("https://example.com/path?name=caf%C3%A9");
            assert_eq!(link.kind, LinkKind::Other);
        }

        #[test]
        fn handles_very_long_url() {
            let long_params = "x".repeat(1000);
            let url = format!("https://zoom.us/j/123?extra={}", long_params);
            let link = detect_link(&url);
            assert_eq!(link.kind, LinkKind::Zoom);
            assert_eq!(link.meeting_id, Some("123".to_string()));
        }
    }
}
