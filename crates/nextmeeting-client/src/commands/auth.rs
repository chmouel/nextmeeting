//! Authentication commands.

use std::path::PathBuf;

use tracing::info;

use crate::config::{ClientConfig, GoogleSettings};
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
/// * `client_id` - OAuth client ID (can be provided via CLI, env, or config)
/// * `client_secret` - OAuth client secret (can be provided via CLI, env, or config)
/// * `credentials_file` - Path to Google Cloud Console credentials JSON file
/// * `domain` - Optional Google Workspace domain
/// * `force` - Force re-authentication even if already authenticated
/// * `config` - Client configuration (for reading saved credentials)
pub async fn google(
    client_id: Option<String>,
    client_secret: Option<String>,
    credentials_file: Option<PathBuf>,
    domain: Option<String>,
    force: bool,
    config: &ClientConfig,
) -> ClientResult<()> {
    use nextmeeting_providers::google::{GoogleConfig, GoogleProvider, OAuthCredentials};

    // Resolve credentials from CLI args, environment, or config
    let (final_client_id, final_client_secret) = resolve_google_credentials(
        client_id,
        client_secret,
        credentials_file,
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

    info!("Google authentication successful");
    println!();
    println!("Authentication successful!");
    println!("Your Google Calendar tokens have been saved.");
    println!();
    println!("You can now use nextmeeting to fetch your calendar events.");

    Ok(())
}

/// Default credentials file name.
const DEFAULT_CREDENTIALS_FILE: &str = "oauth.json";

/// Returns the default path for Google credentials file.
///
/// This is `~/.local/share/nextmeeting/oauth.json` on Linux.
fn default_credentials_path() -> PathBuf {
    crate::config::ClientConfig::default_data_dir().join(DEFAULT_CREDENTIALS_FILE)
}

/// Resolves Google credentials from multiple sources.
///
/// Priority (highest to lowest):
/// 1. CLI `--client-id` + `--client-secret`
/// 2. CLI `--credentials-file`
/// 3. Config inline `client_id` + `client_secret`
/// 4. Config `credentials_file`
/// 5. Default file at `~/.local/share/nextmeeting/oauth.json`
fn resolve_google_credentials(
    cli_client_id: Option<String>,
    cli_client_secret: Option<String>,
    cli_credentials_file: Option<PathBuf>,
    config_google: Option<&GoogleSettings>,
) -> ClientResult<(String, String)> {
    use nextmeeting_providers::google::OAuthCredentials;

    // Priority 1: CLI client_id + client_secret
    if let (Some(id), Some(secret)) = (&cli_client_id, &cli_client_secret) {
        return Ok((id.clone(), secret.clone()));
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
        return Ok((creds.client_id, creds.client_secret));
    }

    // Priority 3: Config inline client_id + client_secret
    if let Some(google) = config_google {
        if !google.client_id.is_empty() && !google.client_secret.is_empty() {
            return Ok((google.client_id.clone(), google.client_secret.clone()));
        }

        // Priority 4: Config credentials_file
        if let Some(ref path) = google.credentials_file {
            let creds = OAuthCredentials::from_file(path).map_err(|e| {
                crate::error::ClientError::Config(format!(
                    "failed to load credentials from {}: {}",
                    path.display(),
                    e
                ))
            })?;
            return Ok((creds.client_id, creds.client_secret));
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
        return Ok((creds.client_id, creds.client_secret));
    }

    // Handle partial CLI args (only id or only secret provided)
    if cli_client_id.is_some() || cli_client_secret.is_some() {
        return Err(crate::error::ClientError::Config(
            "both --client-id and --client-secret are required when providing credentials directly"
                .to_string(),
        ));
    }

    let default_path_str = default_path.display();
    Err(crate::error::ClientError::Config(format!(
        "Google credentials are required. Provide via:\n  \
         - Place credentials JSON at {default_path_str}\n  \
         - --client-id and --client-secret flags\n  \
         - --credentials-file flag (path to Google Cloud Console JSON)\n  \
         - GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET env vars\n  \
         - GOOGLE_CREDENTIALS_FILE env var\n  \
         - config file (client_id/client_secret or credentials_file)"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_credentials_from_cli() {
        let result = resolve_google_credentials(
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            None,
            None,
        );
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "cli-id.apps.googleusercontent.com");
        assert_eq!(secret, "cli-secret");
    }

    #[test]
    fn resolve_credentials_from_config() {
        let google_settings = GoogleSettings {
            client_id: "config-id.apps.googleusercontent.com".to_string(),
            client_secret: "config-secret".to_string(),
            credentials_file: None,
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let result = resolve_google_credentials(None, None, None, Some(&google_settings));
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "config-id.apps.googleusercontent.com");
        assert_eq!(secret, "config-secret");
    }

    #[test]
    fn resolve_credentials_cli_overrides_config() {
        let google_settings = GoogleSettings {
            client_id: "config-id.apps.googleusercontent.com".to_string(),
            client_secret: "config-secret".to_string(),
            credentials_file: None,
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let result = resolve_google_credentials(
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            None,
            Some(&google_settings),
        );
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "cli-id.apps.googleusercontent.com");
        assert_eq!(secret, "cli-secret");
    }

    #[test]
    fn resolve_credentials_partial_cli_fails() {
        // Only client_id without client_secret should fail
        let result = resolve_google_credentials(
            Some("id.apps.googleusercontent.com".to_string()),
            None,
            None,
            None,
        );
        assert!(result.is_err());

        // Only client_secret without client_id should fail
        let result = resolve_google_credentials(None, Some("secret".to_string()), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_credentials_no_credentials_fails() {
        let result = resolve_google_credentials(None, None, None, None);
        assert!(result.is_err());
    }
}
