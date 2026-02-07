//! Link detection and normalization for meeting URLs.
//!
//! This module provides functionality to:
//! - Extract meeting URLs from text (event descriptions, locations)
//! - Unwrap Microsoft Outlook SafeLinks
//! - Detect and classify video conferencing services (67 services)
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
static SAFELINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://[^/]*safelinks\.protection\.outlook\.com/?\?[^?]*url=([^&]+)")
        .expect("Invalid SafeLink regex")
});

/// A service definition mapping a regex pattern to a LinkKind.
struct ServiceDef {
    kind: LinkKind,
    pattern: &'static str,
}

/// Service definitions ordered by specificity (most specific first).
///
/// Patterns are sourced from MeetingBar's MeetingServices.swift, adapted
/// to Rust regex syntax.
const SERVICE_DEFS: &[ServiceDef] = &[
    // --- Protocol-scheme URLs first ---
    ServiceDef {
        kind: LinkKind::ZoomNative,
        pattern: r"zoommtg://([a-z0-9-.]+)?zoom(-x)?\.(?:us|com|com\.cn|de)/join[-a-zA-Z0-9()@:%_\+.~#?&=/]*",
    },
    // --- Specific subdomains / paths before broader domain matches ---
    ServiceDef {
        kind: LinkKind::MeetStream,
        pattern: r"https?://stream\.meet\.google\.com/stream/[a-z0-9-]+",
    },
    ServiceDef {
        kind: LinkKind::ZoomGov,
        pattern: r"https?://([a-z0-9.]+)?zoomgov\.com/j/[a-zA-Z0-9?&=]+",
    },
    ServiceDef {
        kind: LinkKind::Zoom,
        pattern: r"https?://(?:[a-zA-Z0-9-]+\.)?zoom(-x)?\.(?:us|com|com\.cn|de)/(?:my|[a-z]{1,4}|webinar)/?[-a-zA-Z0-9()@:%_\+.~#?&=/]*",
    },
    ServiceDef {
        kind: LinkKind::GoogleMeet,
        pattern: r"https?://meet\.google\.com/(_meet/)?[a-z-]+",
    },
    ServiceDef {
        kind: LinkKind::Teams,
        pattern: r"https?://(gov\.)?teams\.(microsoft\.com|live\.com|microsoft\.us)/l/meetup-join/[a-zA-Z0-9_%/=\-\+\.?]+",
    },
    ServiceDef {
        kind: LinkKind::Webex,
        pattern: r"https?://(?:[A-Za-z0-9-]+\.)?webex\.com(?:(?:/[-A-Za-z0-9]+/j\.php\?MTID=[A-Za-z0-9]+(?:&\S*)?)|(?:/(?:meet|join)/[A-Za-z0-9\-._@]+(?:\?\S*)?))",
    },
    ServiceDef {
        kind: LinkKind::Chime,
        pattern: r"https?://([a-z0-9-.]+)?chime\.aws/[0-9]*",
    },
    ServiceDef {
        kind: LinkKind::Jitsi,
        pattern: r"https?://meet\.jit\.si/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::RingCentral,
        pattern: r"https?://([a-z0-9.]+)?ringcentral\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::GoToMeeting,
        pattern: r"https?://([a-z0-9.]+)?gotomeeting\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::GoToWebinar,
        pattern: r"https?://([a-z0-9.]+)?gotowebinar\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::BlueJeans,
        pattern: r"https?://([a-z0-9.]+)?bluejeans\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::EightByEight,
        pattern: r"https?://8x8\.vc/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Demio,
        pattern: r"https?://event\.demio\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::JoinMe,
        pattern: r"https?://join\.me/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Whereby,
        pattern: r"https?://whereby\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::UberConference,
        pattern: r"https?://uberconference\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Blizz,
        pattern: r"https?://go\.blizz\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::TeamViewerMeeting,
        pattern: r"https?://go\.teamviewer\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::VSee,
        pattern: r"https?://vsee\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::StarLeaf,
        pattern: r"https?://meet\.starleaf\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Duo,
        pattern: r"https?://duo\.app\.goo\.gl/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Voov,
        pattern: r"https?://voovmeeting\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::FacebookWorkplace,
        pattern: r"https?://([a-z0-9-.]+)?workplace\.com/groupcall/[^\s]+",
    },
    ServiceDef {
        kind: LinkKind::Skype,
        pattern: r"https?://join\.skype\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Skype4Biz,
        pattern: r"https?://meet\.lync\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Skype4BizSelfHosted,
        pattern: r"https?://(meet|join)\.[^\s]*/[a-z0-9.]+/meet/[A-Za-z0-9./]+",
    },
    ServiceDef {
        kind: LinkKind::Lifesize,
        pattern: r"https?://call\.lifesizecloud\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::YouTube,
        pattern: r"https?://((www|m)\.)?(youtube\.com|youtu\.be)/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::VonageMeetings,
        pattern: r"https?://meetings\.vonage\.com/[0-9]{9}",
    },
    ServiceDef {
        kind: LinkKind::Around,
        pattern: r"https?://(meet\.)?around\.co/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Jam,
        pattern: r"https?://jam\.systems/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Discord,
        pattern: r"(https?|discord)://(www\.)?(canary\.)?discord(app)?\.([a-zA-Z]{2,})(.+)?",
    },
    ServiceDef {
        kind: LinkKind::BlackboardCollab,
        pattern: r"https?://us\.bbcollab\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::CoScreen,
        pattern: r"https?://join\.coscreen\.co/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Vowel,
        pattern: r"https?://([a-z0-9.]+)?vowel\.com/#/g/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Zhumu,
        pattern: r"https?://welink\.zhumu\.com/j/[0-9]+\?pwd=[a-zA-Z0-9]+",
    },
    ServiceDef {
        kind: LinkKind::Lark,
        pattern: r"https?://vc\.larksuite\.com/j/[0-9]+",
    },
    ServiceDef {
        kind: LinkKind::Feishu,
        pattern: r"https?://vc\.feishu\.cn/j/[0-9]+",
    },
    ServiceDef {
        kind: LinkKind::Vimeo,
        pattern: r"https?://(?:vimeo\.com/(?:showcase|event)/[0-9]+|venues\.vimeo\.com/[^\s]+)",
    },
    ServiceDef {
        kind: LinkKind::Ovice,
        pattern: r"https?://([a-z0-9-.]+)?ovice\.(in|com)/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::FaceTime,
        pattern: r"https?://facetime\.apple\.com/join[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Chorus,
        pattern: r"https?://go\.chorus\.ai/[^\s]+",
    },
    ServiceDef {
        kind: LinkKind::Pop,
        pattern: r"https?://pop\.com/j/[0-9-]+",
    },
    ServiceDef {
        kind: LinkKind::Gong,
        pattern: r"https?://([a-z0-9-.]+)?join\.gong\.io/[^\s]+",
    },
    ServiceDef {
        kind: LinkKind::Livestorm,
        pattern: r"https?://app\.livestorm\.com/p/[^\s]+",
    },
    ServiceDef {
        kind: LinkKind::Luma,
        pattern: r"https?://lu\.ma/join/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Preply,
        pattern: r"https?://preply\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::UserZoom,
        pattern: r"https?://go\.userzoom\.com/participate/[a-z0-9-]+",
    },
    ServiceDef {
        kind: LinkKind::Venue,
        pattern: r"https?://app\.venue\.live/app/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Teemyco,
        pattern: r"https?://app\.teemyco\.com/room/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Demodesk,
        pattern: r"https?://demodesk\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::ZohoCliq,
        pattern: r"https?://cliq\.zoho\.eu/meetings/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Hangouts,
        pattern: r"https?://hangouts\.google\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Slack,
        pattern: r"https?://app\.slack\.com/huddle/[A-Za-z0-9./]+",
    },
    ServiceDef {
        kind: LinkKind::Reclaim,
        pattern: r"https?://reclaim\.ai/z/[A-Za-z0-9./]+",
    },
    ServiceDef {
        kind: LinkKind::Tuple,
        pattern: r"https?://tuple\.app/c/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Gather,
        pattern: r"https?://app\.gather\.town/app/[A-Za-z0-9]+/[A-Za-z0-9_%\-]+\?(spawnToken|meeting)=[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::Pumble,
        pattern: r"https?://meet\.pumble\.com/[a-z-]+",
    },
    ServiceDef {
        kind: LinkKind::SuitConference,
        pattern: r"https?://([a-z0-9.]+)?conference\.istesuit\.com/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::DoxyMe,
        pattern: r"https?://([a-z0-9.]+)?doxy\.me/[^\s]*",
    },
    ServiceDef {
        kind: LinkKind::CalCom,
        pattern: r"https?://app\.cal\.com/video/[A-Za-z0-9./]+",
    },
    ServiceDef {
        kind: LinkKind::ZmPage,
        pattern: r"https?://([a-zA-Z0-9.]+)\.zm\.page",
    },
    ServiceDef {
        kind: LinkKind::LiveKit,
        pattern: r"https?://meet[a-zA-Z0-9.]*\.livekit\.io/rooms/[a-zA-Z0-9-#]+",
    },
    ServiceDef {
        kind: LinkKind::Meetecho,
        pattern: r"https?://meetings\.conf\.meetecho\.com/.+",
    },
    ServiceDef {
        kind: LinkKind::StreamYard,
        pattern: r"https?://(?:www\.)?streamyard\.com/(?:guest/)?([a-z0-9]{8,13})(?:/|\?[^ \n]*)?",
    },
];

/// Compiled service registry: patterns compiled to Regex at first access.
static SERVICE_REGISTRY: LazyLock<Vec<(Regex, LinkKind)>> = LazyLock::new(|| {
    SERVICE_DEFS
        .iter()
        .map(|d| {
            (
                Regex::new(d.pattern).unwrap_or_else(|e| {
                    panic!("Invalid regex for {:?}: {}", d.kind, e);
                }),
                d.kind,
            )
        })
        .collect()
});

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
        let unwrapped = unwrap_safelink(url);

        for (regex, kind) in SERVICE_REGISTRY.iter() {
            if regex.is_match(&unwrapped) {
                return match kind {
                    LinkKind::Zoom => normalize_zoom(&unwrapped, false),
                    LinkKind::ZoomGov => normalize_zoom(&unwrapped, true),
                    LinkKind::ZoomNative => normalize_zoom_native(&unwrapped),
                    LinkKind::GoogleMeet => normalize_meet(&unwrapped),
                    LinkKind::Teams => normalize_teams(&unwrapped),
                    LinkKind::Jitsi => normalize_jitsi(&unwrapped),
                    _ => EventLink::new(*kind, unwrapped.trim()),
                };
            }
        }

        EventLink::new(LinkKind::Other, unwrapped)
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
    if let Some(caps) = SAFELINK_REGEX.captures(url)
        && let Some(encoded) = caps.get(1)
    {
        // URL-decode the original link
        if let Ok(decoded) = urlencoding::decode(encoded.as_str()) {
            return decoded.into_owned();
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

/// Converts a `zoommtg://` protocol URL to a standard `https://zoom.us/j/` URL.
///
/// Extracts `confno` and `pwd` query parameters from the native URL format.
fn normalize_zoom_native(url: &str) -> EventLink {
    // zoommtg:// URLs look like: zoommtg://zoom.us/join?confno=123&pwd=abc
    // Convert to https://zoom.us/j/123?pwd=abc
    let https_url = url
        .replacen("zoommtg://", "https://", 1)
        .replace("/join?", "/j?");

    let Ok(parsed) = Url::parse(&https_url) else {
        return EventLink::new(LinkKind::ZoomNative, url);
    };

    let mut meeting_id: Option<String> = None;
    let mut passcode: Option<String> = None;

    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "confno" => meeting_id = Some(value.into_owned()),
            "pwd" | "passcode" => passcode = Some(value.into_owned()),
            _ => {}
        }
    }

    let normalized = if let Some(ref id) = meeting_id {
        let mut new_url = format!("https://zoom.us/j/{}", id);
        if let Some(ref pwd) = passcode {
            new_url.push_str("?pwd=");
            new_url.push_str(pwd);
        }
        new_url
    } else {
        url.to_string()
    };

    EventLink::with_credentials(LinkKind::ZoomNative, normalized, meeting_id, passcode)
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
        .find(|s| !s.is_empty())
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
        .find(|s| !s.is_empty())
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

        #[test]
        fn handles_zoom_native() {
            let link = detect_link("zoommtg://zoom.us/join?confno=123456789&pwd=secret");
            assert_eq!(link.kind, LinkKind::ZoomNative);
            assert_eq!(link.url, "https://zoom.us/j/123456789?pwd=secret");
            assert_eq!(link.meeting_id, Some("123456789".to_string()));
            assert_eq!(link.passcode, Some("secret".to_string()));
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
            let url = "https://teams.live.com/l/meetup-join/abc123";
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

    mod all_services {
        use super::*;

        #[test]
        fn test_all_services_detected() {
            let cases: &[(&str, LinkKind)] = &[
                // Zoom variants
                ("zoommtg://zoom.us/join?confno=123&pwd=abc", LinkKind::ZoomNative),
                ("https://stream.meet.google.com/stream/abc-123", LinkKind::MeetStream),
                ("https://example.zoomgov.com/j/123456789", LinkKind::ZoomGov),
                ("https://zoom.us/j/123456789", LinkKind::Zoom),
                ("https://company.zoom.us/my/johndoe", LinkKind::Zoom),
                ("https://meet.google.com/abc-defg-hij", LinkKind::GoogleMeet),
                ("https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc@thread.v2/0?context=xyz", LinkKind::Teams),
                ("https://company.webex.com/meet/john.doe", LinkKind::Webex),
                ("https://chime.aws/1234567890", LinkKind::Chime),
                ("https://meet.jit.si/MyRoom", LinkKind::Jitsi),
                ("https://meetings.ringcentral.com/j/123", LinkKind::RingCentral),
                ("https://global.gotomeeting.com/join/123", LinkKind::GoToMeeting),
                ("https://attendee.gotowebinar.com/register/123", LinkKind::GoToWebinar),
                ("https://bluejeans.com/123", LinkKind::BlueJeans),
                ("https://8x8.vc/123/abc", LinkKind::EightByEight),
                ("https://event.demio.com/ref/abc123", LinkKind::Demio),
                ("https://join.me/123-456-789", LinkKind::JoinMe),
                ("https://whereby.com/my-room", LinkKind::Whereby),
                ("https://uberconference.com/room123", LinkKind::UberConference),
                ("https://go.blizz.com/join/123", LinkKind::Blizz),
                ("https://go.teamviewer.com/meet/123", LinkKind::TeamViewerMeeting),
                ("https://vsee.com/c/room123", LinkKind::VSee),
                ("https://meet.starleaf.com/123456", LinkKind::StarLeaf),
                ("https://duo.app.goo.gl/abc123", LinkKind::Duo),
                ("https://voovmeeting.com/dm/abc123", LinkKind::Voov),
                ("https://company.workplace.com/groupcall/123", LinkKind::FacebookWorkplace),
                ("https://join.skype.com/abc123", LinkKind::Skype),
                ("https://meet.lync.com/company/user/abc", LinkKind::Skype4Biz),
                ("https://call.lifesizecloud.com/123456", LinkKind::Lifesize),
                ("https://www.youtube.com/watch?v=abc123", LinkKind::YouTube),
                ("https://meetings.vonage.com/123456789", LinkKind::VonageMeetings),
                ("https://meet.around.co/r/my-room", LinkKind::Around),
                ("https://jam.systems/new-room", LinkKind::Jam),
                ("https://discord.com/channels/123/456", LinkKind::Discord),
                ("https://us.bbcollab.com/invite/abc123", LinkKind::BlackboardCollab),
                ("https://join.coscreen.co/abc123", LinkKind::CoScreen),
                ("https://app.vowel.com/#/g/abc123", LinkKind::Vowel),
                ("https://welink.zhumu.com/j/123456?pwd=abc123", LinkKind::Zhumu),
                ("https://vc.larksuite.com/j/123456789", LinkKind::Lark),
                ("https://vc.feishu.cn/j/123456789", LinkKind::Feishu),
                ("https://vimeo.com/showcase/123456", LinkKind::Vimeo),
                ("https://company.ovice.in/room", LinkKind::Ovice),
                ("https://facetime.apple.com/join#v=1&p=abc", LinkKind::FaceTime),
                ("https://go.chorus.ai/abc123", LinkKind::Chorus),
                ("https://pop.com/j/123-456", LinkKind::Pop),
                ("https://app.join.gong.io/call/abc123", LinkKind::Gong),
                ("https://app.livestorm.com/p/my-event", LinkKind::Livestorm),
                ("https://lu.ma/join/abc123", LinkKind::Luma),
                ("https://preply.com/en/tutor/123", LinkKind::Preply),
                ("https://go.userzoom.com/participate/abc-123", LinkKind::UserZoom),
                ("https://app.venue.live/app/room123", LinkKind::Venue),
                ("https://app.teemyco.com/room/abc123", LinkKind::Teemyco),
                ("https://demodesk.com/meet/abc", LinkKind::Demodesk),
                ("https://cliq.zoho.eu/meetings/abc123", LinkKind::ZohoCliq),
                ("https://hangouts.google.com/call/abc123", LinkKind::Hangouts),
                ("https://app.slack.com/huddle/T123/C456", LinkKind::Slack),
                ("https://reclaim.ai/z/abc123", LinkKind::Reclaim),
                ("https://tuple.app/c/room-123", LinkKind::Tuple),
                ("https://app.gather.town/app/abc123/MySpace?spawnToken=xyz", LinkKind::Gather),
                ("https://meet.pumble.com/my-room", LinkKind::Pumble),
                ("https://my.conference.istesuit.com/room123", LinkKind::SuitConference),
                ("https://doxy.me/drsmith", LinkKind::DoxyMe),
                ("https://app.cal.com/video/abc123", LinkKind::CalCom),
                ("https://company.zm.page", LinkKind::ZmPage),
                ("https://meet.livekit.io/rooms/my-room-123", LinkKind::LiveKit),
                ("https://meetings.conf.meetecho.com/ietf123", LinkKind::Meetecho),
                ("https://streamyard.com/abc12345678", LinkKind::StreamYard),
            ];

            for (url, expected) in cases {
                let link = detect_link(url);
                assert_eq!(
                    link.kind, *expected,
                    "Failed for URL: {} — expected {:?}, got {:?}",
                    url, expected, link.kind
                );
            }
        }
    }
}
