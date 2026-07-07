//! OAuth 2.0 PKCE flow implementation for Google APIs.
//!
//! This module implements the Authorization Code flow with PKCE (Proof Key for
//! Code Exchange) extension, using a loopback redirect for desktop applications.
//!
//! # Flow Overview
//!
//! 1. Generate a cryptographic code verifier and its SHA-256 challenge
//! 2. Start a local HTTP server on a random port
//! 3. Build the authorization URL with the challenge
//! 4. Open the user's browser to Google's consent page
//! 5. User grants permission; Google redirects to our local server
//! 6. Extract the authorization code from the redirect
//! 7. Exchange the code (with verifier) for access and refresh tokens
//!
//! # Security
//!
//! - PKCE prevents authorization code interception attacks
//! - The loopback server only accepts connections from localhost
//! - State parameter prevents CSRF attacks
//! - Tokens are stored with restrictive file permissions

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng as _;
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, warn};

use crate::error::{ProviderError, ProviderResult};

use super::config::OAuthCredentials;
use super::tokens::TokenInfo;

/// Google OAuth endpoints.
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";

/// The PKCE code verifier length (in bytes, before base64 encoding).
const CODE_VERIFIER_LENGTH: usize = 32;

/// Timeout for waiting for the OAuth callback.
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Options controlling the interactive authorization flow.
#[derive(Debug, Clone, Copy)]
pub struct AuthorizeOptions {
    /// Force the Google consent screen (`prompt=consent`).
    ///
    /// When false, consent is only requested when Google deems it
    /// necessary; a retry with consent happens automatically if no
    /// refresh token is returned.
    pub force_consent: bool,

    /// Open the system browser automatically.
    ///
    /// When false (headless/SSH sessions), the authorization URL is
    /// printed and the user may paste the redirect URL back into the
    /// terminal instead of relying on the loopback redirect.
    pub open_browser: bool,
}

impl Default for AuthorizeOptions {
    fn default() -> Self {
        Self {
            force_consent: false,
            open_browser: true,
        }
    }
}

/// OAuth client for Google APIs.
///
/// Handles the OAuth 2.0 PKCE flow for obtaining and refreshing tokens.
#[derive(Debug)]
pub struct OAuthClient {
    credentials: OAuthCredentials,
    http_client: reqwest::Client,
}

impl OAuthClient {
    /// Creates a new OAuth client with the given credentials.
    pub fn new(credentials: OAuthCredentials, timeout: Duration) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to create HTTP client");

        Self {
            credentials,
            http_client,
        }
    }

    /// Initiates the OAuth PKCE flow and returns the obtained tokens.
    ///
    /// This will:
    /// 1. Start a local HTTP server
    /// 2. Open the user's browser to Google's authorization page
    /// 3. Wait for the callback with the authorization code
    /// 4. Exchange the code for tokens
    ///
    /// # Arguments
    ///
    /// * `scopes` - The OAuth scopes to request
    /// * `port_range` - Range of ports to try for the loopback server
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No port is available in the specified range
    /// - The browser cannot be opened
    /// - The user denies authorization
    /// - Token exchange fails
    pub async fn authorize(
        &self,
        scopes: &[String],
        port_range: (u16, u16),
    ) -> ProviderResult<TokenInfo> {
        self.authorize_with_options(scopes, port_range, AuthorizeOptions::default())
            .await
    }

    /// Initiates the OAuth PKCE flow with explicit options.
    ///
    /// See [`authorize`](Self::authorize) for the flow description and
    /// [`AuthorizeOptions`] for the available knobs.
    pub async fn authorize_with_options(
        &self,
        scopes: &[String],
        port_range: (u16, u16),
        options: AuthorizeOptions,
    ) -> ProviderResult<TokenInfo> {
        let tokens = self.run_authorization(scopes, port_range, options).await?;

        // Google only returns a refresh token on consent. If we skipped the
        // consent screen and got no refresh token, retry once with consent
        // so the user is not left with a session that cannot refresh.
        if tokens.refresh_token.is_none() && !options.force_consent {
            info!("no refresh token returned; retrying with consent prompt");
            eprintln!("No refresh token was granted; requesting consent once more...");
            return self
                .run_authorization(
                    scopes,
                    port_range,
                    AuthorizeOptions {
                        force_consent: true,
                        ..options
                    },
                )
                .await;
        }

        Ok(tokens)
    }

    /// Runs a single pass of the authorization flow.
    async fn run_authorization(
        &self,
        scopes: &[String],
        port_range: (u16, u16),
        options: AuthorizeOptions,
    ) -> ProviderResult<TokenInfo> {
        let pkce = PkceFlow::new();

        // Find an available port and start the callback server
        let (listener, port) = Self::bind_loopback_server(port_range)?;
        let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

        // Build the authorization URL
        let auth_url = pkce.build_auth_url(
            &self.credentials.client_id,
            &redirect_uri,
            scopes,
            options.force_consent,
        );

        info!("starting OAuth flow");
        debug!("authorization URL: {}", auth_url);

        let mut manual_entry = !options.open_browser;
        if options.open_browser
            && let Err(e) = open::that(&auth_url)
        {
            warn!("failed to open browser: {}", e);
            manual_entry = true;
        }

        if manual_entry {
            eprintln!("\nOpen this URL in your browser:\n\n{}\n", auth_url);
            eprintln!(
                "After authorising, your browser is redirected to {}.",
                redirect_uri
            );
            eprintln!(
                "If that page cannot be reached (e.g. over SSH), paste the full\n\
                 redirect URL from your browser's address bar here and press Enter."
            );
        } else {
            eprintln!("A browser window has been opened for you to authorise access.");
            eprintln!("Waiting for the redirect on {} ...", redirect_uri);
            eprintln!(
                "(If Google shows 'Error 400: redirect_uri_mismatch', your OAuth\n\
                 client is not of type 'Desktop app' - see `nextmeeting auth google --guide`.)"
            );
        }

        // Wait for the callback (and, in manual mode, a pasted redirect URL)
        let (code, received_state) = Self::wait_for_callback(listener, manual_entry)?;

        // Verify state matches
        if received_state != pkce.state {
            return Err(ProviderError::authentication(
                "OAuth state mismatch - possible CSRF attack",
            ));
        }

        info!("received authorization code, exchanging for tokens...");

        // Exchange the code for tokens
        self.exchange_code(&code, &pkce.verifier, &redirect_uri, scopes)
            .await
    }

    /// Refreshes an expired access token using the refresh token.
    ///
    /// Returns the new access token and its expiry time.
    pub async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> ProviderResult<(String, Option<i64>)> {
        let params = [
            ("client_id", self.credentials.client_id.as_str()),
            ("client_secret", self.credentials.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];

        let response = self
            .http_client
            .post(GOOGLE_TOKEN_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("token refresh request failed: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ProviderError::network(format!("failed to read response: {}", e)))?;

        if !status.is_success() {
            return Err(Self::token_endpoint_error("token refresh", status, &body));
        }

        let token_response: TokenResponse = serde_json::from_str(&body).map_err(|e| {
            ProviderError::invalid_response(format!("invalid token response: {}", e))
        })?;

        info!("successfully refreshed access token");
        Ok((token_response.access_token, token_response.expires_in))
    }

    /// Exchanges an authorization code for tokens.
    async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        scopes: &[String],
    ) -> ProviderResult<TokenInfo> {
        let params = [
            ("client_id", self.credentials.client_id.as_str()),
            ("client_secret", self.credentials.client_secret.as_str()),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ];

        let response = self
            .http_client
            .post(GOOGLE_TOKEN_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("token exchange request failed: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ProviderError::network(format!("failed to read response: {}", e)))?;

        if !status.is_success() {
            return Err(Self::token_endpoint_error("token exchange", status, &body));
        }

        let token_response: TokenResponse = serde_json::from_str(&body).map_err(|e| {
            ProviderError::invalid_response(format!("invalid token response: {}", e))
        })?;

        info!("successfully obtained tokens");
        Ok(TokenInfo::new(
            token_response.access_token,
            token_response.refresh_token,
            token_response.expires_in,
            scopes.to_vec(),
        ))
    }

    /// Maps a token endpoint failure to a helpful provider error.
    ///
    /// `invalid_grant` (expired or revoked refresh token) gets a dedicated
    /// message telling the user how to re-authenticate.
    fn token_endpoint_error(
        operation: &str,
        status: reqwest::StatusCode,
        body: &str,
    ) -> ProviderError {
        #[derive(serde::Deserialize)]
        struct OAuthErrorBody {
            error: Option<String>,
            error_description: Option<String>,
        }

        if let Ok(parsed) = serde_json::from_str::<OAuthErrorBody>(body) {
            if parsed.error.as_deref() == Some("invalid_grant") {
                return ProviderError::authentication(
                    "Google session expired or revoked (invalid_grant) - \
                     run `nextmeeting auth google` to re-authenticate",
                );
            }
            if let Some(error) = parsed.error {
                let description = parsed.error_description.unwrap_or_default();
                return ProviderError::authentication(format!(
                    "{} failed ({}): {} {}",
                    operation, status, error, description
                ));
            }
        }

        ProviderError::authentication(format!("{} failed ({}): {}", operation, status, body))
    }

    /// Revokes a token (access or refresh) at Google's revocation endpoint.
    ///
    /// Revoking a refresh token also invalidates the associated access
    /// tokens.
    pub async fn revoke_token(&self, token: &str) -> ProviderResult<()> {
        let response = self
            .http_client
            .post(GOOGLE_REVOKE_URL)
            .form(&[("token", token)])
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("token revocation failed: {}", e)))?;

        let status = response.status();
        if status.is_success() {
            info!("token revoked");
            Ok(())
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ProviderError::authentication(format!(
                "token revocation failed ({}): {}",
                status, body
            )))
        }
    }

    /// Tries to bind a TCP listener on an available port in the given range.
    ///
    /// A range of `(0, 0)` requests an ephemeral port from the operating
    /// system, which is the recommended mode for Desktop app clients.
    fn bind_loopback_server(port_range: (u16, u16)) -> ProviderResult<(TcpListener, u16)> {
        if port_range == (0, 0) {
            let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| {
                ProviderError::configuration(format!("failed to bind loopback server: {}", e))
            })?;
            let port = listener
                .local_addr()
                .map_err(|e| {
                    ProviderError::internal(format!("failed to read local address: {}", e))
                })?
                .port();
            debug!("bound loopback server on ephemeral port {}", port);
            return Ok((listener, port));
        }

        for port in port_range.0..=port_range.1 {
            match TcpListener::bind(format!("127.0.0.1:{}", port)) {
                Ok(listener) => {
                    debug!("bound loopback server on port {}", port);
                    return Ok((listener, port));
                }
                Err(_) => continue,
            }
        }
        Err(ProviderError::configuration(format!(
            "no available port in range {}-{}",
            port_range.0, port_range.1
        )))
    }

    /// Waits for the OAuth callback and extracts the authorization code.
    ///
    /// In manual mode, a pasted redirect URL on stdin is accepted as an
    /// alternative to the loopback callback.
    fn wait_for_callback(
        listener: TcpListener,
        accept_manual_entry: bool,
    ) -> ProviderResult<(String, String)> {
        listener
            .set_nonblocking(false)
            .map_err(|e| ProviderError::internal(format!("failed to set blocking: {}", e)))?;

        let (tx, rx) = mpsc::channel();

        // Handle the callback in a separate thread to allow timeout
        let callback_tx = tx.clone();
        let _handle = thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        if let Some(result) = Self::handle_callback(stream) {
                            let _ = callback_tx.send(result);
                            return;
                        }
                    }
                    Err(e) => {
                        error!("failed to accept connection: {}", e);
                    }
                }
            }
        });

        // In manual mode, also accept a pasted redirect URL on stdin
        if accept_manual_entry {
            let stdin_tx = tx;
            let _stdin_handle = thread::spawn(move || {
                let stdin = std::io::stdin();
                let mut line = String::new();
                loop {
                    line.clear();
                    match stdin.lock().read_line(&mut line) {
                        Ok(0) => return, // EOF
                        Ok(_) => {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            let _ = stdin_tx.send(Self::parse_pasted_redirect(trimmed));
                            return;
                        }
                        Err(_) => return,
                    }
                }
            });
        }

        // Wait with timeout
        match rx.recv_timeout(CALLBACK_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ProviderError::authentication(
                "OAuth callback timeout - no authorisation was received within 5 minutes. \
                 If the browser showed an error, check that your OAuth client is of \
                 type 'Desktop app' (see `nextmeeting auth google --guide`)",
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(ProviderError::internal("callback channel disconnected"))
            }
        }
    }

    /// Parses a pasted redirect URL (or bare query string) into
    /// `(code, state)`.
    fn parse_pasted_redirect(input: &str) -> ProviderResult<(String, String)> {
        // Accept a full URL, a `/callback?...` path, or a bare query string
        let query = match input.find('?') {
            Some(i) => &input[i + 1..],
            None => input,
        };
        let query = query.trim_end_matches('#');

        let (code, state, error) = Self::parse_query_params(query);

        if let Some(error) = error {
            return Err(ProviderError::authentication(format!(
                "authorization denied: {}",
                error
            )));
        }

        match code {
            Some(c) => Ok((c, state.unwrap_or_default())),
            None => Err(ProviderError::authentication(
                "the pasted URL does not contain an authorization code \
                 (expected a 'code=' parameter)",
            )),
        }
    }

    /// Parses `code`, `state`, and `error` from a query string.
    fn parse_query_params(query: &str) -> (Option<String>, Option<String>, Option<String>) {
        let mut code = None;
        let mut state = None;
        let mut error = None;

        for param in query.split('&') {
            let mut kv = param.splitn(2, '=');
            if let (Some(key), Some(value)) = (kv.next(), kv.next()) {
                match key {
                    "code" => {
                        code = Some(urlencoding::decode(value).unwrap_or_default().into_owned())
                    }
                    "state" => {
                        state = Some(urlencoding::decode(value).unwrap_or_default().into_owned())
                    }
                    "error" => {
                        error = Some(urlencoding::decode(value).unwrap_or_default().into_owned())
                    }
                    _ => {}
                }
            }
        }

        (code, state, error)
    }

    /// Handles an incoming HTTP request on the callback server.
    fn handle_callback(mut stream: TcpStream) -> Option<ProviderResult<(String, String)>> {
        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();

        if reader.read_line(&mut request_line).is_err() {
            return None;
        }

        // Parse the request line: GET /callback?code=...&state=... HTTP/1.1
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 || parts[0] != "GET" {
            return None;
        }

        let path = parts[1];
        if !path.starts_with("/callback") {
            return None;
        }

        // Parse query parameters
        let query_start = path.find('?').map(|i| i + 1).unwrap_or(path.len());
        let query = &path[query_start..];

        let (code, state, error) = Self::parse_query_params(query);

        // Send response to browser
        let response = if error.is_some() || code.is_none() {
            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\n\r\n\
            <html><body><h1>Authorization Failed</h1>\
            <p>You can close this window.</p></body></html>"
        } else {
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
            <html><body><h1>Authorization Successful</h1>\
            <p>You can close this window and return to the terminal.</p></body></html>"
        };

        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();

        // Return result
        if let Some(error) = error {
            return Some(Err(ProviderError::authentication(format!(
                "authorization denied: {}",
                error
            ))));
        }

        match (code, state) {
            (Some(c), Some(s)) => Some(Ok((c, s))),
            (Some(c), None) => Some(Ok((c, String::new()))),
            _ => Some(Err(ProviderError::authentication(
                "missing authorization code in callback",
            ))),
        }
    }
}

/// PKCE flow state and utilities.
///
/// Implements RFC 7636 (Proof Key for Code Exchange).
#[derive(Debug)]
pub struct PkceFlow {
    /// The code verifier (high-entropy random string).
    pub verifier: String,
    /// The code challenge (SHA-256 hash of verifier, base64url encoded).
    pub challenge: String,
    /// Random state for CSRF protection.
    pub state: String,
}

impl PkceFlow {
    /// Creates a new PKCE flow with random verifier and state.
    pub fn new() -> Self {
        let verifier = Self::generate_verifier();
        let challenge = Self::compute_challenge(&verifier);
        let state = Self::generate_state();

        Self {
            verifier,
            challenge,
            state,
        }
    }

    /// Generates a cryptographically random code verifier.
    fn generate_verifier() -> String {
        let mut rng = rand::rng();
        let bytes: Vec<u8> = (0..CODE_VERIFIER_LENGTH).map(|_| rng.random()).collect();
        URL_SAFE_NO_PAD.encode(&bytes)
    }

    /// Computes the SHA-256 challenge for a code verifier.
    fn compute_challenge(verifier: &str) -> String {
        let digest = Sha256::digest(verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }

    /// Generates a random state string for CSRF protection.
    fn generate_state() -> String {
        let mut rng = rand::rng();
        let bytes: Vec<u8> = (0..16).map(|_| rng.random()).collect();
        URL_SAFE_NO_PAD.encode(&bytes)
    }

    /// Builds the Google OAuth authorization URL.
    ///
    /// `force_consent` adds `prompt=consent`, which guarantees a refresh
    /// token at the cost of always showing the consent screen.
    pub fn build_auth_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        scopes: &[String],
        force_consent: bool,
    ) -> String {
        let scope = scopes.join(" ");

        let mut url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&\
            code_challenge={}&code_challenge_method=S256&state={}&\
            access_type=offline",
            GOOGLE_AUTH_URL,
            urlencoding::encode(client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scope),
            urlencoding::encode(&self.challenge),
            urlencoding::encode(&self.state),
        );

        if force_consent {
            url.push_str("&prompt=consent");
        }

        url
    }
}

impl Default for PkceFlow {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from Google's token endpoint.
#[derive(Debug, serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    #[allow(dead_code)]
    token_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_length() {
        let flow = PkceFlow::new();
        // Base64 encoding of 32 bytes = 43 characters (no padding)
        assert_eq!(flow.verifier.len(), 43);
    }

    #[test]
    fn pkce_challenge_is_deterministic() {
        let verifier = "test-verifier-string";
        let challenge1 = PkceFlow::compute_challenge(verifier);
        let challenge2 = PkceFlow::compute_challenge(verifier);
        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn pkce_challenge_differs_for_different_verifiers() {
        let flow1 = PkceFlow::new();
        let flow2 = PkceFlow::new();
        assert_ne!(flow1.challenge, flow2.challenge);
    }

    #[test]
    fn pkce_state_is_random() {
        let flow1 = PkceFlow::new();
        let flow2 = PkceFlow::new();
        assert_ne!(flow1.state, flow2.state);
    }

    #[test]
    fn auth_url_format() {
        let flow = PkceFlow::new();
        let url = flow.build_auth_url(
            "test-client.apps.googleusercontent.com",
            "http://127.0.0.1:8080/callback",
            &["https://www.googleapis.com/auth/calendar.readonly".to_string()],
            false,
        );

        assert!(url.starts_with(GOOGLE_AUTH_URL));
        assert!(url.contains("client_id="));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state="));
        assert!(url.contains("access_type=offline"));
        // Consent is not forced by default
        assert!(!url.contains("prompt=consent"));
    }

    #[test]
    fn auth_url_with_forced_consent() {
        let flow = PkceFlow::new();
        let url = flow.build_auth_url(
            "test-client.apps.googleusercontent.com",
            "http://127.0.0.1:8080/callback",
            &["https://www.googleapis.com/auth/calendar.events".to_string()],
            true,
        );
        assert!(url.contains("prompt=consent"));
    }

    #[test]
    fn parse_pasted_redirect_full_url() {
        let (code, state) = OAuthClient::parse_pasted_redirect(
            "http://127.0.0.1:43217/callback?state=abc&code=4%2FxyzToken&scope=foo",
        )
        .unwrap();
        assert_eq!(code, "4/xyzToken");
        assert_eq!(state, "abc");
    }

    #[test]
    fn parse_pasted_redirect_bare_query() {
        let (code, state) = OAuthClient::parse_pasted_redirect("code=abc123&state=st").unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "st");
    }

    #[test]
    fn parse_pasted_redirect_error_param() {
        let result = OAuthClient::parse_pasted_redirect(
            "http://127.0.0.1:8080/callback?error=access_denied",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("access_denied"));
    }

    #[test]
    fn parse_pasted_redirect_missing_code() {
        let result = OAuthClient::parse_pasted_redirect("https://example.com/?state=only");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("code"));
    }

    #[test]
    fn token_endpoint_error_invalid_grant() {
        let err = OAuthClient::token_endpoint_error(
            "token refresh",
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error": "invalid_grant", "error_description": "Token has been expired or revoked."}"#,
        );
        let message = err.to_string();
        assert!(message.contains("expired or revoked"));
        assert!(message.contains("nextmeeting auth google"));
    }

    #[test]
    fn token_endpoint_error_other_oauth_error() {
        let err = OAuthClient::token_endpoint_error(
            "token exchange",
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error": "invalid_client", "error_description": "Unauthorized"}"#,
        );
        let message = err.to_string();
        assert!(message.contains("invalid_client"));
    }

    #[test]
    fn token_endpoint_error_non_json_body() {
        let err = OAuthClient::token_endpoint_error(
            "token refresh",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "upstream broke",
        );
        assert!(err.to_string().contains("upstream broke"));
    }

    #[test]
    fn bind_ephemeral_port() {
        let (listener, port) = OAuthClient::bind_loopback_server((0, 0)).unwrap();
        assert!(port > 0);
        assert_eq!(listener.local_addr().unwrap().port(), port);
    }
}
