//! Authentication commands.

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
/// * `domain` - Optional Google Workspace domain
/// * `force` - Force re-authentication even if already authenticated
/// * `config` - Client configuration (for reading saved credentials)
pub async fn google(
    client_id: Option<String>,
    client_secret: Option<String>,
    domain: Option<String>,
    force: bool,
    config: &ClientConfig,
) -> ClientResult<()> {
    use nextmeeting_providers::google::{GoogleConfig, GoogleProvider, OAuthCredentials};

    // Resolve credentials from CLI args, environment, or config
    let (final_client_id, final_client_secret) = resolve_google_credentials(
        client_id,
        client_secret,
        config.google.as_ref(),
    )?;

    // Build provider configuration
    let credentials = OAuthCredentials::new(&final_client_id, &final_client_secret);
    credentials.validate().map_err(|e| {
        crate::error::ClientError::Config(format!("invalid Google credentials: {}", e))
    })?;

    let mut google_config = GoogleConfig::new(credentials);

    // Apply domain from CLI or config
    let final_domain = domain.or_else(|| {
        config
            .google
            .as_ref()
            .and_then(|g| g.domain.clone())
    });
    if let Some(d) = final_domain {
        google_config = google_config.with_domain(d);
    }

    // Apply token path from config
    if let Some(ref google_settings) = config.google {
        if let Some(ref path) = google_settings.token_path {
            google_config = google_config.with_token_path(path);
        }
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

/// Resolves Google credentials from multiple sources.
///
/// Priority (highest to lowest):
/// 1. CLI arguments
/// 2. Environment variables (handled by clap)
/// 3. Configuration file
fn resolve_google_credentials(
    cli_client_id: Option<String>,
    cli_client_secret: Option<String>,
    config_google: Option<&GoogleSettings>,
) -> ClientResult<(String, String)> {
    let client_id = cli_client_id
        .or_else(|| config_google.map(|g| g.client_id.clone()))
        .ok_or_else(|| {
            crate::error::ClientError::Config(
                "Google client_id is required. Provide via --client-id, \
                GOOGLE_CLIENT_ID env var, or config file."
                    .to_string(),
            )
        })?;

    let client_secret = cli_client_secret
        .or_else(|| config_google.map(|g| g.client_secret.clone()))
        .ok_or_else(|| {
            crate::error::ClientError::Config(
                "Google client_secret is required. Provide via --client-secret, \
                GOOGLE_CLIENT_SECRET env var, or config file."
                    .to_string(),
            )
        })?;

    Ok((client_id, client_secret))
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
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let result = resolve_google_credentials(None, None, Some(&google_settings));
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
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let result = resolve_google_credentials(
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            Some(&google_settings),
        );
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "cli-id.apps.googleusercontent.com");
        assert_eq!(secret, "cli-secret");
    }

    #[test]
    fn resolve_credentials_missing_id() {
        let result = resolve_google_credentials(None, Some("secret".to_string()), None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_credentials_missing_secret() {
        let result = resolve_google_credentials(
            Some("id.apps.googleusercontent.com".to_string()),
            None,
            None,
        );
        assert!(result.is_err());
    }
}
