//! OAuth token storage and management.
//!
//! This module handles secure storage and retrieval of OAuth tokens,
//! as well as token refresh logic.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::error::{ProviderError, ProviderResult};

/// Information about an OAuth token set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// The access token for API requests.
    pub access_token: String,

    /// The refresh token for obtaining new access tokens.
    pub refresh_token: Option<String>,

    /// When the access token expires.
    pub expires_at: Option<DateTime<Utc>>,

    /// The OAuth scopes that were granted.
    pub scopes: Vec<String>,

    /// When the tokens were last refreshed.
    pub last_refresh: DateTime<Utc>,
}

impl TokenInfo {
    /// Creates a new token info from OAuth response data.
    pub fn new(
        access_token: impl Into<String>,
        refresh_token: Option<String>,
        expires_in_secs: Option<i64>,
        scopes: Vec<String>,
    ) -> Self {
        let expires_at = expires_in_secs.map(|secs| {
            // Subtract a buffer to refresh before actual expiry
            Utc::now() + Duration::seconds(secs) - Duration::seconds(60)
        });

        Self {
            access_token: access_token.into(),
            refresh_token,
            expires_at,
            scopes,
            last_refresh: Utc::now(),
        }
    }

    /// Returns true if the access token is expired or about to expire.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expires_at) => Utc::now() >= expires_at,
            // If no expiry is set, assume it's valid (some tokens don't expire)
            None => false,
        }
    }

    /// Returns true if the token has (or satisfies) the required scopes.
    ///
    /// Uses scope-satisfaction logic: the full `calendar` scope counts as
    /// covering the narrower calendar scopes.
    pub fn has_scopes(&self, required: &[String]) -> bool {
        super::config::GoogleConfig::scopes_satisfied(&self.scopes, required)
    }

    /// Updates the access token after a refresh.
    pub fn update_access_token(
        &mut self,
        access_token: impl Into<String>,
        expires_in_secs: Option<i64>,
    ) {
        self.access_token = access_token.into();
        self.expires_at = expires_in_secs
            .map(|secs| Utc::now() + Duration::seconds(secs) - Duration::seconds(60));
        self.last_refresh = Utc::now();
    }

    /// Returns the time until the token expires, if known.
    pub fn time_until_expiry(&self) -> Option<Duration> {
        self.expires_at.map(|expires_at| expires_at - Utc::now())
    }
}

/// Storage backend for OAuth tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenBackend {
    /// JSON file with 0600 permissions (default).
    #[default]
    File,
    /// Desktop keyring via the freedesktop Secret Service (`secret-tool`).
    ///
    /// Falls back to file storage with a warning when no secret service
    /// is available.
    Keyring,
}

impl TokenBackend {
    /// Parses a backend name from configuration (`"file"` or `"keyring"`).
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "file" => Ok(Self::File),
            "keyring" => Ok(Self::Keyring),
            other => Err(format!(
                "unknown token storage backend '{}'; expected 'file' or 'keyring'",
                other
            )),
        }
    }

    /// Returns the backend name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Keyring => "keyring",
        }
    }
}

/// Persisted token storage with file-based backend.
///
/// Tokens are stored as JSON in the user's config directory, or in the
/// desktop keyring when the [`TokenBackend::Keyring`] backend is selected.
/// The storage handles reading, writing, and updating tokens atomically.
#[derive(Debug)]
pub struct TokenStorage {
    /// Path to the token file (also the fallback for the keyring backend).
    path: PathBuf,

    /// The selected storage backend.
    backend: TokenBackend,

    /// Keyring attribute identifying this account (`google-<account>`).
    keyring_account: Option<String>,

    /// The `secret-tool` binary to invoke (overridable for tests).
    secret_tool: String,

    /// In-memory cache of the current tokens.
    tokens: RwLock<Option<TokenInfo>>,
}

/// Keyring service attribute for all nextmeeting secrets.
const KEYRING_SERVICE: &str = "nextmeeting";

impl TokenStorage {
    /// Creates a new file-backed token storage at the given path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            backend: TokenBackend::File,
            keyring_account: None,
            secret_tool: "secret-tool".to_string(),
            tokens: RwLock::new(None),
        }
    }

    /// Creates a keyring-backed token storage.
    ///
    /// `fallback_path` is used when no secret service is available.
    pub fn new_keyring(account: impl Into<String>, fallback_path: impl Into<PathBuf>) -> Self {
        Self {
            path: fallback_path.into(),
            backend: TokenBackend::Keyring,
            keyring_account: Some(format!("google-{}", account.into())),
            secret_tool: "secret-tool".to_string(),
            tokens: RwLock::new(None),
        }
    }

    /// Overrides the `secret-tool` binary (for tests).
    #[doc(hidden)]
    pub fn with_secret_tool(mut self, command: impl Into<String>) -> Self {
        self.secret_tool = command.into();
        self
    }

    /// Returns the selected backend.
    pub fn backend(&self) -> TokenBackend {
        self.backend
    }

    /// Looks up tokens in the keyring.
    ///
    /// Returns `Ok(None)` when no entry exists, `Err` when the secret
    /// service is unavailable.
    fn keyring_lookup(&self) -> Result<Option<String>, String> {
        let account = self.keyring_account.as_deref().unwrap_or_default();
        let output = std::process::Command::new(&self.secret_tool)
            .args(["lookup", "service", KEYRING_SERVICE, "account", account])
            .output()
            .map_err(|e| format!("failed to run {}: {}", self.secret_tool, e))?;

        if !output.status.success() {
            // secret-tool exits non-zero when no matching secret exists
            return Ok(None);
        }
        let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if content.is_empty() {
            Ok(None)
        } else {
            Ok(Some(content))
        }
    }

    /// Stores tokens in the keyring.
    fn keyring_store(&self, content: &str) -> Result<(), String> {
        use std::io::Write as _;
        let account = self.keyring_account.as_deref().unwrap_or_default();
        let mut child = std::process::Command::new(&self.secret_tool)
            .args([
                "store",
                "--label",
                &format!("nextmeeting Google tokens ({})", account),
                "service",
                KEYRING_SERVICE,
                "account",
                account,
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to run {}: {}", self.secret_tool, e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(content.as_bytes())
                .map_err(|e| format!("failed to write to {}: {}", self.secret_tool, e))?;
        }

        let status = child
            .wait()
            .map_err(|e| format!("failed to wait for {}: {}", self.secret_tool, e))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "{} store failed (exit {})",
                self.secret_tool, status
            ))
        }
    }

    /// Removes tokens from the keyring.
    fn keyring_clear(&self) {
        let account = self.keyring_account.as_deref().unwrap_or_default();
        let _ = std::process::Command::new(&self.secret_tool)
            .args(["clear", "service", KEYRING_SERVICE, "account", account])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    /// Loads tokens from the backend into memory.
    ///
    /// Returns Ok(true) if tokens were loaded, Ok(false) if no tokens exist.
    pub fn load(&self) -> ProviderResult<bool> {
        if self.backend == TokenBackend::Keyring {
            match self.keyring_lookup() {
                Ok(Some(content)) => {
                    let tokens: TokenInfo = serde_json::from_str(&content).map_err(|e| {
                        ProviderError::configuration(format!(
                            "failed to parse keyring tokens: {}",
                            e
                        ))
                    })?;
                    info!("loaded tokens from keyring");
                    *self.tokens.write().unwrap() = Some(tokens);
                    return Ok(true);
                }
                Ok(None) => {
                    // No keyring entry; fall through to the file for migration
                    debug!("no keyring entry, checking token file");
                }
                Err(e) => {
                    warn!(
                        "keyring unavailable ({}); falling back to file storage at {:?}",
                        e, self.path
                    );
                }
            }
        }

        self.load_from_file()
    }

    /// Loads tokens from the file backend.
    fn load_from_file(&self) -> ProviderResult<bool> {
        if !self.path.exists() {
            debug!("no token file at {:?}", self.path);
            return Ok(false);
        }

        let content = fs::read_to_string(&self.path).map_err(|e| {
            ProviderError::configuration(format!("failed to read token file: {}", e))
        })?;

        let tokens: TokenInfo = serde_json::from_str(&content).map_err(|e| {
            ProviderError::configuration(format!("failed to parse token file: {}", e))
        })?;

        info!("loaded tokens from {:?}", self.path);
        *self.tokens.write().unwrap() = Some(tokens);
        Ok(true)
    }

    /// Saves the current tokens to the backend.
    pub fn save(&self) -> ProviderResult<()> {
        let content = {
            let tokens = self.tokens.read().unwrap();
            let tokens = tokens
                .as_ref()
                .ok_or_else(|| ProviderError::internal("no tokens to save"))?;
            serde_json::to_string_pretty(tokens).map_err(|e| {
                ProviderError::internal(format!("failed to serialize tokens: {}", e))
            })?
        };

        if self.backend == TokenBackend::Keyring {
            match self.keyring_store(&content) {
                Ok(()) => {
                    debug!("saved tokens to keyring");
                    // Remove any stale fallback file so secrets live in one place
                    if self.path.exists() {
                        let _ = fs::remove_file(&self.path);
                    }
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        "keyring unavailable ({}); falling back to file storage at {:?}",
                        e, self.path
                    );
                }
            }
        }

        self.save_to_file(&content)
    }

    /// Saves serialized tokens to the file backend.
    fn save_to_file(&self, content: &str) -> ProviderResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ProviderError::configuration(format!("failed to create token directory: {}", e))
            })?;
        }

        // Write to temp file first, then rename for atomicity
        let temp_path = self.path.with_extension("json.tmp");

        fs::write(&temp_path, content).map_err(|e| {
            ProviderError::configuration(format!("failed to write token file: {}", e))
        })?;

        fs::rename(&temp_path, &self.path).map_err(|e| {
            ProviderError::configuration(format!("failed to rename token file: {}", e))
        })?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            let _ = fs::set_permissions(&self.path, perms);
        }

        debug!("saved tokens to {:?}", self.path);
        Ok(())
    }

    /// Returns a clone of the current tokens, if any.
    pub fn get(&self) -> Option<TokenInfo> {
        self.tokens.read().unwrap().clone()
    }

    /// Sets new tokens and saves them to disk.
    pub fn set(&self, tokens: TokenInfo) -> ProviderResult<()> {
        *self.tokens.write().unwrap() = Some(tokens);
        self.save()
    }

    /// Updates the access token and saves to disk.
    pub fn update_access_token(
        &self,
        access_token: impl Into<String>,
        expires_in_secs: Option<i64>,
    ) -> ProviderResult<()> {
        let mut tokens = self.tokens.write().unwrap();
        if let Some(ref mut t) = *tokens {
            t.update_access_token(access_token, expires_in_secs);
            drop(tokens);
            self.save()
        } else {
            Err(ProviderError::internal("no tokens to update"))
        }
    }

    /// Clears the stored tokens (in memory, on disk, and in the keyring).
    pub fn clear(&self) -> ProviderResult<()> {
        *self.tokens.write().unwrap() = None;
        if self.backend == TokenBackend::Keyring {
            self.keyring_clear();
        }
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|e| {
                ProviderError::configuration(format!("failed to remove token file: {}", e))
            })?;
            info!("cleared tokens from {:?}", self.path);
        }
        Ok(())
    }

    /// Returns the token storage path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns true if tokens are loaded and not expired.
    pub fn has_valid_tokens(&self) -> bool {
        self.tokens
            .read()
            .unwrap()
            .as_ref()
            .is_some_and(|t| !t.is_expired())
    }

    /// Returns true if tokens are loaded and have a refresh token.
    pub fn has_refresh_token(&self) -> bool {
        self.tokens
            .read()
            .unwrap()
            .as_ref()
            .is_some_and(|t| t.refresh_token.is_some())
    }

    /// Returns true if the stored tokens have the required scopes.
    pub fn has_scopes(&self, required: &[String]) -> bool {
        self.tokens
            .read()
            .unwrap()
            .as_ref()
            .is_some_and(|t| t.has_scopes(required))
    }

    /// Checks if re-authentication is needed due to scope changes.
    ///
    /// Returns true if the required scopes are not present in the stored tokens.
    pub fn needs_reauth(&self, required_scopes: &[String]) -> bool {
        match self.tokens.read().unwrap().as_ref() {
            None => true,
            Some(tokens) => !tokens.has_scopes(required_scopes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> PathBuf {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut path = env::temp_dir();
        path.push(format!(
            "nextmeeting-test-tokens-{}-{}.json",
            std::process::id(),
            counter
        ));
        path
    }

    #[test]
    fn token_info_creation() {
        let token = TokenInfo::new(
            "access-token",
            Some("refresh-token".to_string()),
            Some(3600),
            vec!["scope1".to_string()],
        );

        assert_eq!(token.access_token, "access-token");
        assert_eq!(token.refresh_token, Some("refresh-token".to_string()));
        assert!(token.expires_at.is_some());
        assert!(!token.is_expired());
    }

    #[test]
    fn token_info_expired() {
        let mut token = TokenInfo::new("access", None, Some(3600), vec![]);
        // Force expiry in the past
        token.expires_at = Some(Utc::now() - Duration::hours(1));
        assert!(token.is_expired());
    }

    #[test]
    fn token_info_scope_check() {
        let token = TokenInfo::new(
            "access",
            None,
            None,
            vec!["scope1".to_string(), "scope2".to_string()],
        );

        assert!(token.has_scopes(&["scope1".to_string()]));
        assert!(token.has_scopes(&["scope1".to_string(), "scope2".to_string()]));
        assert!(!token.has_scopes(&["scope3".to_string()]));
    }

    #[test]
    fn token_storage_save_and_load() {
        let path = temp_path();
        let storage = TokenStorage::new(path.clone());

        let token = TokenInfo::new(
            "access-token",
            Some("refresh-token".to_string()),
            Some(3600),
            vec!["scope1".to_string()],
        );

        storage.set(token).unwrap();
        assert!(path.exists());

        // Create new storage and load
        let storage2 = TokenStorage::new(path.clone());
        assert!(storage2.load().unwrap());
        let loaded = storage2.get().unwrap();
        assert_eq!(loaded.access_token, "access-token");

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn token_storage_clear() {
        let path = temp_path();
        let storage = TokenStorage::new(path.clone());

        let token = TokenInfo::new("access", None, None, vec![]);
        storage.set(token).unwrap();
        assert!(path.exists());

        storage.clear().unwrap();
        assert!(!path.exists());
        assert!(storage.get().is_none());
    }

    #[test]
    fn token_storage_no_file() {
        let path = temp_path();
        let storage = TokenStorage::new(path);
        assert!(!storage.load().unwrap());
        assert!(storage.get().is_none());
    }

    #[test]
    fn token_backend_parse() {
        assert_eq!(TokenBackend::parse("file").unwrap(), TokenBackend::File);
        assert_eq!(
            TokenBackend::parse("keyring").unwrap(),
            TokenBackend::Keyring
        );
        assert!(TokenBackend::parse("vault").is_err());
        assert_eq!(TokenBackend::Keyring.as_str(), "keyring");
    }

    #[test]
    fn token_scope_satisfaction_full_scope() {
        // A token granted the full calendar scope satisfies narrower scopes
        let token = TokenInfo::new(
            "access",
            None,
            None,
            vec!["https://www.googleapis.com/auth/calendar".to_string()],
        );
        assert!(token.has_scopes(&[
            "https://www.googleapis.com/auth/calendar.readonly".to_string(),
            "https://www.googleapis.com/auth/calendar.events".to_string(),
        ]));
    }

    #[test]
    fn keyring_storage_falls_back_to_file() {
        // `false` exits non-zero: lookup treats it as "no entry",
        // store treats it as unavailable and falls back to the file.
        let path = temp_path();
        let storage = TokenStorage::new_keyring("test", path.clone()).with_secret_tool("false");
        assert_eq!(storage.backend(), TokenBackend::Keyring);

        let token = TokenInfo::new("access", None, None, vec![]);
        storage.set(token).unwrap();
        assert!(path.exists(), "fallback file should be written");

        let storage2 = TokenStorage::new_keyring("test", path.clone()).with_secret_tool("false");
        assert!(storage2.load().unwrap());
        assert_eq!(storage2.get().unwrap().access_token, "access");

        storage2.clear().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn keyring_storage_missing_tool_falls_back() {
        let path = temp_path();
        let storage = TokenStorage::new_keyring("test", path.clone())
            .with_secret_tool("/nonexistent/secret-tool");

        let token = TokenInfo::new("access", None, None, vec![]);
        storage.set(token).unwrap();
        assert!(path.exists());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn token_storage_needs_reauth() {
        let path = temp_path();
        let storage = TokenStorage::new(path.clone());

        // No tokens = needs reauth
        assert!(storage.needs_reauth(&["scope1".to_string()]));

        // With matching scopes
        let token = TokenInfo::new("access", None, None, vec!["scope1".to_string()]);
        storage.set(token).unwrap();
        assert!(!storage.needs_reauth(&["scope1".to_string()]));

        // Missing scope = needs reauth
        assert!(storage.needs_reauth(&["scope2".to_string()]));

        // Cleanup
        let _ = fs::remove_file(&path);
    }
}
