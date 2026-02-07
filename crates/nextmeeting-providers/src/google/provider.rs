//! Google Calendar provider implementation.
//!
//! This module implements the [`CalendarProvider`] trait for Google Calendar.

use std::sync::RwLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock as TokioRwLock;
use tracing::{debug, info};

use crate::error::{ProviderError, ProviderResult};
use crate::provider::{
    BoxFuture, CalendarInfo, CalendarProvider, FetchOptions, FetchResult, ProviderStatus,
};

use super::client::GoogleCalendarClient;
use super::config::GoogleConfig;
use super::oauth::OAuthClient;
use super::tokens::TokenStorage;

/// Google Calendar provider.
///
/// This provider fetches events from Google Calendar using the Calendar API v3.
/// It handles authentication via OAuth 2.0 PKCE flow.
pub struct GoogleProvider {
    config: GoogleConfig,
    display_name: String,
    token_storage: TokenStorage,
    oauth_client: OAuthClient,
    /// API client wrapped in tokio RwLock for async access
    api_client: TokioRwLock<Option<GoogleCalendarClient>>,
    last_sync: RwLock<Option<DateTime<Utc>>>,
    last_etag: RwLock<Option<String>>,
}

impl GoogleProvider {
    /// Creates a new Google provider with the given configuration.
    ///
    /// This loads any existing tokens from storage but does not
    /// initiate authentication. Call [`authenticate`] if needed.
    pub fn new(config: GoogleConfig) -> ProviderResult<Self> {
        config.validate().map_err(ProviderError::configuration)?;

        let display_name = config.provider_name();
        let token_storage = TokenStorage::new(&config.token_path);
        let _ = token_storage.load();

        let oauth_client = OAuthClient::new(config.credentials.clone(), config.timeout);

        // Create API client if we have valid tokens
        let api_client = if let Some(tokens) = token_storage.get() {
            if !tokens.is_expired() {
                Some(GoogleCalendarClient::new(
                    &tokens.access_token,
                    config.timeout,
                ))
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            config,
            display_name,
            token_storage,
            oauth_client,
            api_client: TokioRwLock::new(api_client),
            last_sync: RwLock::new(None),
            last_etag: RwLock::new(None),
        })
    }

    /// Initiates the OAuth authentication flow.
    ///
    /// This opens the user's browser to Google's consent page.
    /// After authorization, tokens are stored for future use.
    pub async fn authenticate(&self) -> ProviderResult<()> {
        info!("starting Google authentication flow");

        let tokens = self
            .oauth_client
            .authorize(&self.config.scopes, self.config.loopback_port_range)
            .await?;

        // Store the tokens
        self.token_storage.set(tokens.clone())?;

        // Create the API client
        let client = GoogleCalendarClient::new(&tokens.access_token, self.config.timeout);
        *self.api_client.write().await = Some(client);

        info!("authentication successful");
        Ok(())
    }

    /// Checks if re-authentication is needed (e.g., scope changes).
    pub fn needs_reauth(&self) -> bool {
        self.token_storage.needs_reauth(&self.config.scopes)
    }

    /// Ensures we have a valid API client, refreshing tokens if needed.
    async fn ensure_client(&self) -> ProviderResult<()> {
        // Check if we have a valid client with non-expired tokens
        {
            let client = self.api_client.read().await;
            if client.is_some()
                && let Some(tokens) = self.token_storage.get()
                && !tokens.is_expired()
            {
                return Ok(());
            }
        }

        // Need to refresh or re-authenticate
        self.ensure_authenticated().await
    }

    /// Ensures we have valid authentication, refreshing if needed.
    async fn ensure_authenticated(&self) -> ProviderResult<()> {
        let tokens = self.token_storage.get().ok_or_else(|| {
            ProviderError::authentication("not authenticated - run 'nextmeeting auth google'")
        })?;

        // If token is expired, try to refresh
        if tokens.is_expired() {
            let refresh_token = tokens.refresh_token.as_ref().ok_or_else(|| {
                ProviderError::authentication("no refresh token - re-authentication required")
            })?;

            debug!("refreshing expired access token");

            let (new_access_token, expires_in) =
                self.oauth_client.refresh_token(refresh_token).await?;

            // Update storage
            self.token_storage
                .update_access_token(&new_access_token, expires_in)?;

            // Update or create API client
            let mut client = self.api_client.write().await;
            match client.as_mut() {
                Some(c) => c.set_access_token(&new_access_token),
                None => {
                    *client = Some(GoogleCalendarClient::new(
                        &new_access_token,
                        self.config.timeout,
                    ));
                }
            }
        } else {
            // We have valid tokens but maybe no client - ensure one exists
            let mut client = self.api_client.write().await;
            if client.is_none() {
                *client = Some(GoogleCalendarClient::new(
                    &tokens.access_token,
                    self.config.timeout,
                ));
            }
        }

        Ok(())
    }

    /// Fetches events from all configured calendars.
    async fn fetch_all_calendars(&self, options: &FetchOptions) -> ProviderResult<FetchResult> {
        self.ensure_client().await?;

        // Determine time window
        let now = Utc::now();
        let (time_min, time_max) = match &options.time_window {
            Some(window) => (window.start, window.end),
            None => {
                // Default: 12 hours ago to 48 hours ahead
                let time_min = now - chrono::Duration::hours(12);
                let time_max = now + chrono::Duration::hours(48);
                (time_min, time_max)
            }
        };

        // Determine which calendars to fetch
        let calendar_ids: Vec<String> = options
            .calendar_ids
            .clone()
            .unwrap_or_else(|| self.config.calendar_ids.clone());

        // Get ETag for conditional fetch
        let etag = options
            .if_none_match
            .clone()
            .or_else(|| self.last_etag.read().unwrap().clone());

        let mut all_events = Vec::new();
        let mut new_etag = None;

        for calendar_id in &calendar_ids {
            debug!("fetching events from calendar: {}", calendar_id);

            // Acquire read lock only for the API call, then release
            let (events, response_etag, not_modified) = {
                let client = self.api_client.read().await;
                let client = client
                    .as_ref()
                    .ok_or_else(|| ProviderError::internal("API client not available"))?;

                client
                    .list_events(
                        calendar_id,
                        time_min,
                        time_max,
                        options.max_results,
                        options.expand_recurring,
                        etag.as_deref(),
                    )
                    .await?
            };

            if not_modified {
                debug!("calendar {} not modified", calendar_id);
                return Ok(FetchResult::not_modified());
            }

            all_events.extend(events);
            if new_etag.is_none() {
                new_etag = response_etag;
            }
        }

        // Update state
        *self.last_sync.write().unwrap() = Some(Utc::now());
        if let Some(ref etag) = new_etag {
            *self.last_etag.write().unwrap() = Some(etag.clone());
        }

        let mut result = FetchResult::with_events(all_events);
        if let Some(etag) = new_etag {
            result = result.with_sync_token(etag);
        }

        Ok(result)
    }

    /// Lists available calendars.
    async fn list_calendars_impl(&self) -> ProviderResult<Vec<CalendarInfo>> {
        self.ensure_client().await?;

        let client = self.api_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| ProviderError::internal("API client not available"))?;

        let calendars = client.list_calendars().await?;

        Ok(calendars
            .into_iter()
            .map(|c| {
                let mut info = CalendarInfo::new(&c.id, &c.summary).with_primary(c.primary);

                if let Some(tz) = c.time_zone {
                    info = info.with_timezone(tz);
                }

                info.description = c.description;
                info.background_color = c.background_color;
                info.foreground_color = c.foreground_color;

                info
            })
            .collect())
    }
}

impl CalendarProvider for GoogleProvider {
    fn name(&self) -> &str {
        &self.display_name
    }

    fn fetch_events(&self, options: FetchOptions) -> BoxFuture<'_, ProviderResult<FetchResult>> {
        Box::pin(async move { self.fetch_all_calendars(&options).await })
    }

    fn list_calendars(&self) -> BoxFuture<'_, ProviderResult<Vec<CalendarInfo>>> {
        Box::pin(async move { self.list_calendars_impl().await })
    }

    fn status(&self) -> BoxFuture<'_, ProviderStatus> {
        Box::pin(async move {
            let mut status = ProviderStatus::new(&self.display_name);

            status.is_authenticated = self.is_authenticated();
            status.last_sync = *self.last_sync.read().unwrap();

            if let Some(tokens) = self.token_storage.get() {
                if tokens.is_expired() && tokens.refresh_token.is_none() {
                    status.error = Some("tokens expired and no refresh token".to_string());
                }
            } else {
                status.error = Some("not authenticated".to_string());
            }

            // Try to get calendar count
            if status.is_authenticated
                && let Ok(calendars) = self.list_calendars_impl().await
            {
                status.calendar_count = calendars.len();
            }

            status
        })
    }

    fn refresh_auth(&self) -> BoxFuture<'_, ProviderResult<()>> {
        Box::pin(async move { self.ensure_authenticated().await })
    }

    fn is_authenticated(&self) -> bool {
        if let Some(tokens) = self.token_storage.get() {
            // We're authenticated if we have valid tokens or a refresh token
            !tokens.is_expired() || tokens.refresh_token.is_some()
        } else {
            false
        }
    }

    fn suggested_poll_interval(&self) -> Duration {
        // Google recommends polling no more frequently than once per minute
        Duration::from_secs(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::config::OAuthCredentials;

    fn test_config() -> GoogleConfig {
        let credentials =
            OAuthCredentials::new("test-client.apps.googleusercontent.com", "test-secret");
        GoogleConfig::new(credentials).with_token_path("/tmp/nonexistent-tokens.json")
    }

    #[test]
    fn provider_creation() {
        let config = test_config();
        let provider = GoogleProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn provider_name() {
        let config = test_config();
        let provider = GoogleProvider::new(config).unwrap();
        assert_eq!(provider.name(), "google:default");
    }

    #[test]
    fn provider_name_with_account() {
        let credentials =
            OAuthCredentials::new("test-client.apps.googleusercontent.com", "test-secret");
        let config = GoogleConfig::new(credentials)
            .with_account_name("work")
            .with_token_path("/tmp/nonexistent-tokens.json");
        let provider = GoogleProvider::new(config).unwrap();
        assert_eq!(provider.name(), "google:work");
    }

    #[test]
    fn provider_not_authenticated_initially() {
        let config = test_config();
        let provider = GoogleProvider::new(config).unwrap();
        assert!(!provider.is_authenticated());
    }

    #[test]
    fn provider_needs_reauth_without_tokens() {
        let config = test_config();
        let provider = GoogleProvider::new(config).unwrap();
        assert!(provider.needs_reauth());
    }

    #[test]
    fn provider_suggested_interval() {
        let config = test_config();
        let provider = GoogleProvider::new(config).unwrap();
        assert_eq!(provider.suggested_poll_interval(), Duration::from_secs(60));
    }
}
