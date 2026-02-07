//! Authentication commands.

use std::path::PathBuf;

use tracing::info;

use crate::config::{AuthConfig, ClientConfig, GoogleAuthCredentials, GoogleSettings};
use crate::error::ClientResult;

// Import the trait to access is_authenticated method
use nextmeeting_providers::CalendarProvider;

/// Run the Google authentication flow.
///
/// This command initiates the OAuth 2.0 PKCE flow to authenticate with
/// Google Calendar. It will:
///
/// 1. Open the user's browser to Google's consent page
/// 2. Wait for the user to grant permissions
/// 3. Store the resulting tokens for future use
///
/// # Arguments
///
/// * `client_id` - OAuth client ID (can be provided via CLI or env)
/// * `client_secret` - OAuth client secret (can be provided via CLI or env)
/// * `credentials_file` - Path to Google Cloud Console credentials JSON file
/// * `domain` - Optional Google Workspace domain
/// * `force` - Force re-authentication even if already authenticated
/// * `config` - Client configuration
/// * `auth` - Auth configuration (from auth.yaml)
pub async fn google(
    client_id: Option<String>,
    client_secret: Option<String>,
    credentials_file: Option<PathBuf>,
    domain: Option<String>,
    force: bool,
    config: &ClientConfig,
    auth: &AuthConfig,
) -> ClientResult<()> {
    use nextmeeting_providers::google::{GoogleConfig, GoogleProvider, OAuthCredentials};

    // Resolve credentials from CLI args, auth.yaml, or credentials file
    let (final_client_id, final_client_secret, source) = resolve_google_credentials(
        client_id,
        client_secret,
        credentials_file,
        auth,
        config.google.as_ref(),
    )?;

    // Build provider configuration
    let credentials = OAuthCredentials::new(&final_client_id, &final_client_secret);
    credentials.validate().map_err(|e| {
        crate::error::ClientError::Config(format!("invalid Google credentials: {}", e))
    })?;

    let mut google_config = GoogleConfig::new(credentials);

    // Apply domain from CLI or config
    let final_domain = domain.or_else(|| config.google.as_ref().and_then(|g| g.domain.clone()));
    if let Some(d) = final_domain {
        google_config = google_config.with_domain(d);
    }

    // Apply token path from config
    if let Some(ref google_settings) = config.google
        && let Some(ref path) = google_settings.token_path
    {
        google_config = google_config.with_token_path(path);
    }

    // Create the provider
    let provider = GoogleProvider::new(google_config)?;

    // Check if already authenticated
    if provider.is_authenticated() && !force {
        // Save credentials to auth.yaml if they came from CLI or credentials file
        save_credentials_to_auth_yaml(&final_client_id, &final_client_secret, &source);
        println!("Already authenticated with Google Calendar.");
        println!("Use --force to re-authenticate.");
        return Ok(());
    }

    // Perform authentication
    println!("Starting Google Calendar authentication...");
    println!();
    println!("A browser window will open for you to authorize access.");
    println!("If the browser doesn't open, check the terminal for a URL to copy.");
    println!();

    provider.authenticate().await?;

    // Save credentials to auth.yaml so the server can find them
    save_credentials_to_auth_yaml(&final_client_id, &final_client_secret, &source);

    info!("Google authentication successful");
    println!();
    println!("Authentication successful!");
    println!("Your Google Calendar tokens have been saved.");
    println!();
    println!("You can now use nextmeeting to fetch your calendar events.");

    Ok(())
}

/// Where the credentials were resolved from.
#[derive(Debug, PartialEq)]
enum CredentialSource {
    /// From CLI flags (--client-id/--client-secret or --credentials-file)
    Cli,
    /// From auth.yaml (already persisted)
    AuthYaml,
    /// From config.toml credentials_file
    ConfigFile,
    /// From the default oauth.json fallback
    DefaultFile,
}

/// Saves credentials to `auth.yaml` so the server can auto-detect them.
///
/// Only saves if the credentials came from a transient source (CLI flags,
/// credentials file, or default oauth.json). If they're already in auth.yaml,
/// this is a no-op.
fn save_credentials_to_auth_yaml(
    client_id: &str,
    client_secret: &str,
    source: &CredentialSource,
) {
    if *source == CredentialSource::AuthYaml {
        // Already persisted in auth.yaml
        return;
    }

    let auth_config = AuthConfig {
        #[cfg(feature = "google")]
        google: Some(GoogleAuthCredentials {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
        }),
    };

    let auth_path = AuthConfig::default_path();
    match auth_config.save() {
        Ok(()) => {
            info!("Credentials saved to {}", auth_path.display());
            println!("Credentials saved to {}", auth_path.display());
        }
        Err(e) => {
            info!("could not save credentials to {}: {}", auth_path.display(), e);
        }
    }
}

/// Resolves Google credentials from multiple sources.
///
/// Priority (highest to lowest):
/// 1. CLI `--client-id` + `--client-secret`
/// 2. CLI `--credentials-file`
/// 3. `auth.yaml` (`client_id` + `client_secret`)
/// 4. Config `credentials_file`
/// 5. Default file at `~/.local/share/nextmeeting/oauth.json`
fn resolve_google_credentials(
    cli_client_id: Option<String>,
    cli_client_secret: Option<String>,
    cli_credentials_file: Option<PathBuf>,
    auth: &AuthConfig,
    config_google: Option<&GoogleSettings>,
) -> ClientResult<(String, String, CredentialSource)> {
    use nextmeeting_providers::google::OAuthCredentials;

    // Priority 1: CLI client_id + client_secret
    if let (Some(id), Some(secret)) = (&cli_client_id, &cli_client_secret) {
        return Ok((id.clone(), secret.clone(), CredentialSource::Cli));
    }

    // Priority 2: CLI credentials file
    if let Some(ref path) = cli_credentials_file {
        let creds = OAuthCredentials::from_file(path).map_err(|e| {
            crate::error::ClientError::Config(format!(
                "failed to load credentials from {}: {}",
                path.display(),
                e
            ))
        })?;
        return Ok((creds.client_id, creds.client_secret, CredentialSource::Cli));
    }

    // Priority 3: auth.yaml client_id + client_secret
    #[cfg(feature = "google")]
    if let Some(ref google_auth) = auth.google {
        if !google_auth.client_id.is_empty() && !google_auth.client_secret.is_empty() {
            return Ok((
                google_auth.client_id.clone(),
                google_auth.client_secret.clone(),
                CredentialSource::AuthYaml,
            ));
        }
    }

    // Priority 4: Config credentials_file
    if let Some(google) = config_google {
        if let Some(ref path) = google.credentials_file {
            let creds = OAuthCredentials::from_file(path).map_err(|e| {
                crate::error::ClientError::Config(format!(
                    "failed to load credentials from {}: {}",
                    path.display(),
                    e
                ))
            })?;
            return Ok((creds.client_id, creds.client_secret, CredentialSource::ConfigFile));
        }
    }

    // Priority 5: Default credentials file
    let default_path = default_credentials_path();
    if default_path.exists() {
        let creds = OAuthCredentials::from_file(&default_path).map_err(|e| {
            crate::error::ClientError::Config(format!(
                "failed to load credentials from {}: {}",
                default_path.display(),
                e
            ))
        })?;
        return Ok((creds.client_id, creds.client_secret, CredentialSource::DefaultFile));
    }

    // Handle partial CLI args (only id or only secret provided)
    if cli_client_id.is_some() || cli_client_secret.is_some() {
        return Err(crate::error::ClientError::Config(
            "both --client-id and --client-secret are required when providing credentials directly"
                .to_string(),
        ));
    }

    let auth_path = AuthConfig::default_path();
    Err(crate::error::ClientError::Config(format!(
        "Google credentials are required. Provide via:\n  \
         - client_id + client_secret in {}\n  \
         - --client-id and --client-secret flags\n  \
         - --credentials-file flag (path to Google Cloud Console JSON)\n  \
         - GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET env vars\n  \
         - credentials_file in config.toml [google] section",
        auth_path.display()
    )))
}

/// Default credentials file name (legacy fallback).
const DEFAULT_CREDENTIALS_FILE: &str = "oauth.json";

/// Returns the default path for Google credentials file (legacy fallback).
///
/// This is `~/.local/share/nextmeeting/oauth.json` on Linux.
fn default_credentials_path() -> PathBuf {
    crate::config::ClientConfig::default_data_dir().join(DEFAULT_CREDENTIALS_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_with_creds(client_id: &str, client_secret: &str) -> AuthConfig {
        AuthConfig {
            google: Some(GoogleAuthCredentials {
                client_id: client_id.to_string(),
                client_secret: client_secret.to_string(),
            }),
        }
    }

    fn auth_empty() -> AuthConfig {
        AuthConfig::default()
    }

    #[test]
    fn resolve_credentials_from_cli() {
        let auth = auth_empty();
        let result = resolve_google_credentials(
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            None,
            &auth,
            None,
        );
        assert!(result.is_ok());
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "cli-id.apps.googleusercontent.com");
        assert_eq!(secret, "cli-secret");
        assert_eq!(source, CredentialSource::Cli);
    }

    #[test]
    fn resolve_credentials_from_auth_yaml() {
        let auth = auth_with_creds(
            "auth-id.apps.googleusercontent.com",
            "auth-secret",
        );
        let result = resolve_google_credentials(None, None, None, &auth, None);
        assert!(result.is_ok());
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "auth-id.apps.googleusercontent.com");
        assert_eq!(secret, "auth-secret");
        assert_eq!(source, CredentialSource::AuthYaml);
    }

    #[test]
    fn resolve_credentials_cli_overrides_auth_yaml() {
        let auth = auth_with_creds(
            "auth-id.apps.googleusercontent.com",
            "auth-secret",
        );
        let result = resolve_google_credentials(
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            None,
            &auth,
            None,
        );
        assert!(result.is_ok());
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "cli-id.apps.googleusercontent.com");
        assert_eq!(secret, "cli-secret");
        assert_eq!(source, CredentialSource::Cli);
    }

    #[test]
    fn resolve_credentials_partial_cli_fails() {
        // Skip if default credentials file exists (would be used as fallback)
        if default_credentials_path().exists() {
            return;
        }

        let auth = auth_empty();

        // Only client_id without client_secret should fail
        let result = resolve_google_credentials(
            Some("id.apps.googleusercontent.com".to_string()),
            None,
            None,
            &auth,
            None,
        );
        assert!(result.is_err());

        // Only client_secret without client_id should fail
        let result =
            resolve_google_credentials(None, Some("secret".to_string()), None, &auth, None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_credentials_no_credentials_fails() {
        // Skip if default credentials file exists (would be used as fallback)
        if default_credentials_path().exists() {
            return;
        }
        let auth = auth_empty();
        let result = resolve_google_credentials(None, None, None, &auth, None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_credentials_from_cli_credentials_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = tmp.path().join("creds.json");
        std::fs::write(
            &creds_path,
            r#"{
                "installed": {
                    "client_id": "file-id.apps.googleusercontent.com",
                    "client_secret": "file-secret"
                }
            }"#,
        )
        .unwrap();

        let auth = auth_empty();
        let result = resolve_google_credentials(None, None, Some(creds_path), &auth, None);
        assert!(result.is_ok());
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "file-id.apps.googleusercontent.com");
        assert_eq!(secret, "file-secret");
        assert_eq!(source, CredentialSource::Cli);
    }

    #[test]
    fn resolve_credentials_from_config_credentials_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = tmp.path().join("creds.json");
        std::fs::write(
            &creds_path,
            r#"{
                "installed": {
                    "client_id": "config-file-id.apps.googleusercontent.com",
                    "client_secret": "config-file-secret"
                }
            }"#,
        )
        .unwrap();

        let auth = auth_empty();
        let settings = GoogleSettings {
            credentials_file: Some(creds_path),
            ..Default::default()
        };
        let result = resolve_google_credentials(None, None, None, &auth, Some(&settings));
        assert!(result.is_ok());
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "config-file-id.apps.googleusercontent.com");
        assert_eq!(secret, "config-file-secret");
        assert_eq!(source, CredentialSource::ConfigFile);
    }

    #[test]
    fn save_credentials_to_auth_yaml_writes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let auth_path = tmp.path().join("auth.yaml");

        let auth = AuthConfig {
            google: Some(GoogleAuthCredentials {
                client_id: "test.apps.googleusercontent.com".to_string(),
                client_secret: "test-secret".to_string(),
            }),
        };
        auth.save_to(&auth_path).unwrap();

        let loaded = AuthConfig::load_from(&auth_path).unwrap();
        let google = loaded.google.unwrap();
        assert_eq!(google.client_id, "test.apps.googleusercontent.com");
        assert_eq!(google.client_secret, "test-secret");
    }

    #[test]
    fn save_skips_when_source_is_auth_yaml() {
        // This just verifies the no-op path doesn't panic
        save_credentials_to_auth_yaml("id", "secret", &CredentialSource::AuthYaml);
    }
}
