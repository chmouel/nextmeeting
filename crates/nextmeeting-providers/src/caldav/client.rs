//! HTTP client for CalDAV operations.
//!
//! This module provides the low-level HTTP client that handles:
//! - Basic and Digest authentication
//! - PROPFIND and REPORT methods
//! - TLS configuration

use reqwest::{Client, Method, Response, StatusCode};
use tracing::{debug, trace, warn};

use crate::error::{ProviderError, ProviderResult};

use super::auth::{DigestAuth, basic_auth};
use super::config::CalDavConfig;

/// HTTP client for CalDAV operations.
pub struct CalDavClient {
    /// The underlying HTTP client.
    client: Client,
    /// Configuration.
    config: CalDavConfig,
    /// Cached digest auth state (for authentication continuity).
    digest_auth: Option<DigestAuth>,
}

impl CalDavClient {
    /// Creates a new CalDAV client with the given configuration.
    pub fn new(config: CalDavConfig) -> ProviderResult<Self> {
        let client = Client::builder()
            .danger_accept_invalid_certs(!config.verify_tls)
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| ProviderError::network(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            config,
            digest_auth: None,
        })
    }

    /// Performs a PROPFIND request.
    ///
    /// Used for calendar discovery and property retrieval.
    pub async fn propfind(&mut self, url: &str, body: &str, depth: u8) -> ProviderResult<String> {
        self.request("PROPFIND", url, Some(body), Some(depth)).await
    }

    /// Performs a REPORT request.
    ///
    /// Used for calendar-query and calendar-multiget.
    pub async fn report(&mut self, url: &str, body: &str) -> ProviderResult<String> {
        self.request("REPORT", url, Some(body), Some(1)).await
    }

    /// Performs a GET request.
    pub async fn get(&mut self, url: &str) -> ProviderResult<String> {
        self.request("GET", url, None, None).await
    }

    /// Performs an HTTP request with optional authentication retry.
    async fn request(
        &mut self,
        method: &str,
        url: &str,
        body: Option<&str>,
        depth: Option<u8>,
    ) -> ProviderResult<String> {
        // First attempt
        let response = self.send_request(method, url, body, depth).await?;

        // Check if we need to authenticate
        if response.status() == StatusCode::UNAUTHORIZED {
            // Extract the WWW-Authenticate header
            let www_auth = response
                .headers()
                .get("www-authenticate")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            if let Some(auth_header) = www_auth {
                debug!("Received 401, attempting authentication");

                // Try Digest auth first
                if auth_header.starts_with("Digest ") {
                    if let Some(digest) = DigestAuth::parse(&auth_header) {
                        self.digest_auth = Some(digest);
                        return self
                            .send_authenticated_request(method, url, body, depth)
                            .await;
                    }
                }

                // Fall back to Basic auth
                if auth_header.contains("Basic") || self.config.has_credentials() {
                    return self
                        .send_authenticated_request(method, url, body, depth)
                        .await;
                }

                return Err(ProviderError::authentication(
                    "Server requires authentication but no valid method found",
                ));
            }
        }

        self.handle_response(response).await
    }

    /// Sends a request without authentication.
    async fn send_request(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        depth: Option<u8>,
    ) -> ProviderResult<Response> {
        let http_method = Method::from_bytes(method.as_bytes())
            .map_err(|_| ProviderError::internal(format!("Invalid HTTP method: {}", method)))?;

        let mut request = self.client.request(http_method, url);

        // Set Content-Type for XML bodies
        if body.is_some() {
            request = request.header("Content-Type", "application/xml; charset=utf-8");
        }

        // Set Depth header for PROPFIND/REPORT
        if let Some(d) = depth {
            request = request.header("Depth", d.to_string());
        }

        // Add body if present
        if let Some(b) = body {
            request = request.body(b.to_string());
        }

        trace!(method = %method, url = %url, "Sending request");

        request
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("Request failed: {}", e)))
    }

    /// Sends an authenticated request.
    async fn send_authenticated_request(
        &mut self,
        method: &str,
        url: &str,
        body: Option<&str>,
        depth: Option<u8>,
    ) -> ProviderResult<String> {
        let (username, password) = match (&self.config.username, &self.config.password) {
            (Some(u), Some(p)) => (u.clone(), p.clone()),
            _ => {
                return Err(ProviderError::authentication(
                    "Credentials required but not configured",
                ));
            }
        };

        let http_method = Method::from_bytes(method.as_bytes())
            .map_err(|_| ProviderError::internal(format!("Invalid HTTP method: {}", method)))?;

        let mut request = self.client.request(http_method, url);

        // Set Content-Type for XML bodies
        if body.is_some() {
            request = request.header("Content-Type", "application/xml; charset=utf-8");
        }

        // Set Depth header for PROPFIND/REPORT
        if let Some(d) = depth {
            request = request.header("Depth", d.to_string());
        }

        // Add authentication header
        let uri_path = url::Url::parse(url)
            .map(|u| u.path().to_string())
            .unwrap_or_else(|_| url.to_string());

        let auth_header = if let Some(ref mut digest) = self.digest_auth {
            digest.authorize(method, &uri_path, &username, &password)
        } else {
            basic_auth(&username, &password)
        };

        request = request.header("Authorization", auth_header);

        // Add body if present
        if let Some(b) = body {
            request = request.body(b.to_string());
        }

        trace!(method = %method, url = %url, "Sending authenticated request");

        let response = request
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("Authenticated request failed: {}", e)))?;

        self.handle_response(response).await
    }

    /// Handles the HTTP response and extracts the body.
    async fn handle_response(&self, response: Response) -> ProviderResult<String> {
        let status = response.status();
        trace!(status = %status, "Received response");

        match status {
            StatusCode::OK | StatusCode::MULTI_STATUS => response
                .text()
                .await
                .map_err(|e| ProviderError::network(format!("Failed to read response: {}", e))),
            StatusCode::UNAUTHORIZED => Err(ProviderError::authentication(
                "Authentication failed: invalid credentials",
            )),
            StatusCode::FORBIDDEN => Err(ProviderError::authorization("Access denied to calendar")),
            StatusCode::NOT_FOUND => {
                Err(ProviderError::not_found("Calendar or resource not found"))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                Err(ProviderError::rate_limited("Too many requests to server"))
            }
            s if s.is_server_error() => {
                let body = response.text().await.unwrap_or_default();
                Err(ProviderError::server(format!(
                    "Server error ({}): {}",
                    s, body
                )))
            }
            s => {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %s, body = %body, "Unexpected response status");
                Err(ProviderError::invalid_response(format!(
                    "Unexpected status {}: {}",
                    s, body
                )))
            }
        }
    }

    /// Returns the base URL from the configuration.
    pub fn base_url(&self) -> &str {
        self.config.url_str()
    }

    /// Returns the configuration.
    pub fn config(&self) -> &CalDavConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn client_creation() {
        let config = CalDavConfig::new("https://caldav.example.com/")
            .unwrap()
            .with_credentials("user", "pass")
            .with_timeout(Duration::from_secs(10));

        let client = CalDavClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn client_base_url() {
        let config = CalDavConfig::new("https://caldav.example.com/calendars/").unwrap();
        let client = CalDavClient::new(config).unwrap();
        assert_eq!(client.base_url(), "https://caldav.example.com/calendars/");
    }
}
