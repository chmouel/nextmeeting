//! OAuth token storage and management.
//!
//! This module handles secure storage and retrieval of OAuth tokens,
//! as well as token refresh logic.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

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

    /// Returns true if the token has the required scopes.
    pub fn has_scopes(&self, required: &[String]) -> bool {
        required.iter().all(|scope| self.scopes.contains(scope))
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

/// Persisted token storage with file-based backend.
///
/// Tokens are stored as JSON in the user's config directory.
/// The storage handles reading, writing, and updating tokens atomically.
#[derive(Debug)]
pub struct TokenStorage {
    /// Path to the token file.
    path: PathBuf,

    /// In-memory cache of the current tokens.
    tokens: RwLock<Option<TokenInfo>>,
}

impl TokenStorage {
    /// Creates a new token storage at the given path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            tokens: RwLock::new(None),
        }
    }

    /// Loads tokens from disk into memory.
    ///
    /// Returns Ok(true) if tokens were loaded, Ok(false) if no tokens exist.
    pub fn load(&self) -> ProviderResult<bool> {
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

    /// Saves the current tokens to disk.
    pub fn save(&self) -> ProviderResult<()> {
        let tokens = self.tokens.read().unwrap();
        let tokens = tokens
            .as_ref()
            .ok_or_else(|| ProviderError::internal("no tokens to save"))?;

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ProviderError::configuration(format!("failed to create token directory: {}", e))
            })?;
        }

        // Write to temp file first, then rename for atomicity
        let temp_path = self.path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(tokens)
            .map_err(|e| ProviderError::internal(format!("failed to serialize tokens: {}", e)))?;

        fs::write(&temp_path, &content).map_err(|e| {
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

    /// Clears the stored tokens (both in memory and on disk).
    pub fn clear(&self) -> ProviderResult<()> {
        *self.tokens.write().unwrap() = None;
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
