//! CalDAV calendar provider implementation.

use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::error::{ProviderError, ProviderResult};
use crate::provider::{
    BoxFuture, CalendarInfo, CalendarProvider, FetchOptions, FetchResult, ProviderStatus,
};
use crate::raw_event::RawEvent;

use super::client::CalDavClient;
use super::config::CalDavConfig;
use super::ics::parse_ics_content;
use super::xml::{
    DiscoveredCalendar, calendar_query_body, parse_propfind_response, parse_report_response,
    propfind_calendars_body,
};

/// CalDAV calendar provider.
///
/// Fetches events from CalDAV-compatible calendar servers using WebDAV/CalDAV protocols.
pub struct CalDavProvider {
    /// HTTP client for CalDAV operations.
    client: Mutex<CalDavClient>,
    /// Provider configuration.
    config: CalDavConfig,
    /// Whether we've successfully authenticated.
    authenticated: AtomicBool,
    /// Last sync time.
    last_sync: Mutex<Option<chrono::DateTime<Utc>>>,
    /// Cached calendars.
    calendars: Mutex<Vec<DiscoveredCalendar>>,
}

impl CalDavProvider {
    /// Creates a new CalDAV provider with the given configuration.
    pub fn new(config: CalDavConfig) -> ProviderResult<Self> {
        let client = CalDavClient::new(config.clone())?;

        Ok(Self {
            client: Mutex::new(client),
            config,
            authenticated: AtomicBool::new(false),
            last_sync: Mutex::new(None),
            calendars: Mutex::new(Vec::new()),
        })
    }

    /// Discovers calendars at the configured URL.
    async fn discover_calendars(&self) -> ProviderResult<Vec<DiscoveredCalendar>> {
        let url = self.config.url_str();
        let body = propfind_calendars_body();

        debug!(url = %url, "Discovering calendars via PROPFIND");

        let response = {
            let mut client = self.client.lock().await;
            client.propfind(url, &body, 1).await?
        };

        let calendars = parse_propfind_response(&response);

        if calendars.is_empty() {
            // The URL might be a direct calendar URL, not a principal
            // Try to use it directly
            debug!("No calendars found via PROPFIND, assuming direct calendar URL");
            return Ok(vec![DiscoveredCalendar {
                href: url.to_string(),
                display_name: None,
                color: None,
                description: None,
                ctag: None,
            }]);
        }

        info!(count = calendars.len(), "Discovered calendars");

        // Cache the discovered calendars
        {
            let mut cached = self.calendars.lock().await;
            *cached = calendars.clone();
        }

        Ok(calendars)
    }

    /// Fetches events from a specific calendar.
    async fn fetch_calendar_events(
        &self,
        calendar_url: &str,
        options: &FetchOptions,
    ) -> ProviderResult<Vec<RawEvent>> {
        // Calculate time window
        let now = Utc::now();
        let (start, end) = if let Some(ref window) = options.time_window {
            (window.start, window.end)
        } else {
            // Default: lookbehind to lookahead
            let start = now - Duration::hours(self.config.lookbehind_hours as i64);
            let end = now + Duration::hours(self.config.lookahead_hours as i64);
            (start, end)
        };

        debug!(
            calendar = %calendar_url,
            start = %start,
            end = %end,
            "Fetching events with REPORT"
        );

        let query_body = calendar_query_body(start, end);

        let response = {
            let mut client = self.client.lock().await;
            client.report(calendar_url, &query_body).await?
        };

        // Parse the multistatus response
        let event_data = parse_report_response(&response);

        debug!(count = event_data.len(), "Received event responses");

        // Parse each ICS content
        let mut events = Vec::new();
        for (href, _etag, ics) in event_data {
            let calendar_id = calendar_url.to_string();
            let parsed = parse_ics_content(&ics, &calendar_id);

            // Add href as extra data for each event
            for mut event in parsed {
                event.extra.insert("href".to_string(), href.clone());
                events.push(event);
            }
        }

        // Filter out cancelled events
        let events: Vec<_> = events.into_iter().filter(|e| !e.is_cancelled()).collect();

        info!(
            calendar = %calendar_url,
            count = events.len(),
            "Fetched and parsed events"
        );

        Ok(events)
    }

    /// Resolves the calendar URL(s) to use.
    ///
    /// This considers:
    /// 1. If a calendar_hint is set, find matching calendar
    /// 2. If calendar_ids are in options, use those
    /// 3. Otherwise, use all discovered calendars or the base URL directly
    async fn resolve_calendar_urls(&self, options: &FetchOptions) -> ProviderResult<Vec<String>> {
        // If specific calendar IDs are requested in options
        if let Some(ref ids) = options.calendar_ids {
            return Ok(ids.clone());
        }

        // Discover available calendars
        let calendars = self.discover_calendars().await?;

        // If a calendar hint is configured, filter by it
        if let Some(ref hint) = self.config.calendar_hint {
            let hint_lower = hint.to_lowercase();
            let matching: Vec<_> = calendars
                .iter()
                .filter(|c| {
                    c.display_name
                        .as_ref()
                        .is_some_and(|n| n.to_lowercase().contains(&hint_lower))
                        || c.href.to_lowercase().contains(&hint_lower)
                })
                .map(|c| resolve_href(&self.config.url, &c.href))
                .collect();

            if !matching.is_empty() {
                return Ok(matching);
            }

            warn!(
                hint = %hint,
                "Calendar hint did not match any discovered calendars"
            );
        }

        // Use all discovered calendars
        Ok(calendars
            .iter()
            .map(|c| resolve_href(&self.config.url, &c.href))
            .collect())
    }
}

impl CalendarProvider for CalDavProvider {
    fn name(&self) -> &str {
        "caldav"
    }

    fn fetch_events(&self, options: FetchOptions) -> BoxFuture<'_, ProviderResult<FetchResult>> {
        Box::pin(async move {
            // Resolve which calendars to fetch
            let calendar_urls = self.resolve_calendar_urls(&options).await?;

            if calendar_urls.is_empty() {
                return Err(ProviderError::calendar("No calendars found to fetch"));
            }

            let mut all_events = Vec::new();

            // Fetch from each calendar
            for url in calendar_urls {
                match self.fetch_calendar_events(&url, &options).await {
                    Ok(events) => all_events.extend(events),
                    Err(e) => {
                        warn!(calendar = %url, error = %e, "Failed to fetch calendar");
                        // Continue with other calendars
                    }
                }
            }

            // Mark as authenticated if we got here successfully
            self.authenticated.store(true, Ordering::SeqCst);

            // Update last sync time
            {
                let mut last = self.last_sync.lock().await;
                *last = Some(Utc::now());
            }

            // Apply max_results limit if specified
            if let Some(max) = options.max_results {
                all_events.truncate(max);
            }

            Ok(FetchResult::with_events(all_events))
        })
    }

    fn list_calendars(&self) -> BoxFuture<'_, ProviderResult<Vec<CalendarInfo>>> {
        Box::pin(async move {
            let calendars = self.discover_calendars().await?;

            Ok(calendars
                .into_iter()
                .map(|c| {
                    let id = c.href.clone();
                    let name = c.display_name.unwrap_or_else(|| c.href.clone());
                    CalendarInfo::new(id, name)
                })
                .collect())
        })
    }

    fn status(&self) -> BoxFuture<'_, ProviderStatus> {
        Box::pin(async move {
            let mut status = ProviderStatus::new("caldav");
            status.is_authenticated = self.authenticated.load(Ordering::SeqCst);
            status.last_sync = *self.last_sync.lock().await;
            status.calendar_count = self.calendars.lock().await.len();
            status
        })
    }

    fn refresh_auth(&self) -> BoxFuture<'_, ProviderResult<()>> {
        // CalDAV doesn't have token refresh - credentials are validated on each request
        Box::pin(async move { Ok(()) })
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }

    fn suggested_poll_interval(&self) -> std::time::Duration {
        // CalDAV servers typically don't have rate limits like Google
        // We can poll more frequently
        std::time::Duration::from_secs(60)
    }
}

/// Resolves a relative href against a base URL.
fn resolve_href(base: &url::Url, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else {
        base.join(href)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_creation() {
        let config = CalDavConfig::new("https://caldav.example.com/calendars/user/").unwrap();
        let provider = CalDavProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn provider_name() {
        let config = CalDavConfig::new("https://caldav.example.com/").unwrap();
        let provider = CalDavProvider::new(config).unwrap();
        assert_eq!(provider.name(), "caldav");
    }

    #[test]
    fn resolve_relative_href() {
        let base = url::Url::parse("https://caldav.example.com/calendars/user/").unwrap();

        // Relative href
        assert_eq!(
            resolve_href(&base, "work/"),
            "https://caldav.example.com/calendars/user/work/"
        );

        // Absolute path
        assert_eq!(
            resolve_href(&base, "/calendars/user/personal/"),
            "https://caldav.example.com/calendars/user/personal/"
        );

        // Full URL (unchanged)
        assert_eq!(
            resolve_href(&base, "https://other.example.com/cal/"),
            "https://other.example.com/cal/"
        );
    }

    #[test]
    fn initial_status() {
        let config = CalDavConfig::new("https://caldav.example.com/").unwrap();
        let provider = CalDavProvider::new(config).unwrap();

        assert!(!provider.is_authenticated());
    }
}
