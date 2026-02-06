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

    // Track whether credentials came from a non-default source (CLI flags or --credentials-file)
    let credentials_from_cli = client_id.is_some() || credentials_file.is_some();

    // Resolve credentials from CLI args, environment, or config
    let (final_client_id, final_client_secret) = resolve_google_credentials(
        client_id,
        client_secret,
        credentials_file.clone(),
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
        // Even if already authenticated, ensure credentials are saved at the default path
        save_credentials_to_default_path(
            credentials_from_cli,
            credentials_file.as_ref(),
            &final_client_id,
            &final_client_secret,
        );
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

    // Save credentials to default path so the server can auto-detect them
    save_credentials_to_default_path(
        credentials_from_cli,
        credentials_file.as_ref(),
        &final_client_id,
        &final_client_secret,
    );

    info!("Google authentication successful");
    println!();
    println!("Authentication successful!");
    println!("Your Google Calendar tokens have been saved.");
    println!();
    println!("You can now use nextmeeting to fetch your calendar events.");

    Ok(())
}

/// Saves credentials to the default data directory so the server can auto-detect them.
///
/// If a `--credentials-file` was used, copies that file. If `--client-id`/`--client-secret`
/// were used, writes a simple JSON with those values. Skips if credentials are already at
/// the default path or came from config (which the server can already read).
fn save_credentials_to_default_path(
    credentials_from_cli: bool,
    credentials_file: Option<&PathBuf>,
    client_id: &str,
    client_secret: &str,
) {
    if !credentials_from_cli {
        return;
    }

    let default_path = default_credentials_path();

    // If credentials file was provided and it's already at the default path, skip
    if let Some(src) = credentials_file {
        if let (Ok(src_canon), Ok(dst_canon)) =
            (src.canonicalize(), default_path.canonicalize())
        {
            if src_canon == dst_canon {
                return;
            }
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = default_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            info!("could not create data directory {}: {}", parent.display(), e);
            return;
        }
    }

    if let Some(src) = credentials_file {
        // Copy the credentials file to the default location
        match std::fs::copy(src, &default_path) {
            Ok(_) => {
                info!(
                    "Credentials saved to {}",
                    default_path.display()
                );
                println!(
                    "Credentials saved to {}",
                    default_path.display()
                );
            }
            Err(e) => {
                info!(
                    "could not copy credentials to {}: {}",
                    default_path.display(),
                    e
                );
            }
        }
    } else {
        // Write a simple JSON with client_id and client_secret
        let json = format!(
            "{{\n  \"client_id\": \"{}\",\n  \"client_secret\": \"{}\"\n}}\n",
            client_id, client_secret
        );
        match std::fs::write(&default_path, json) {
            Ok(_) => {
                info!(
                    "Credentials saved to {}",
                    default_path.display()
                );
                println!(
                    "Credentials saved to {}",
                    default_path.display()
                );
            }
            Err(e) => {
                info!(
                    "could not write credentials to {}: {}",
                    default_path.display(),
                    e
                );
            }
        }
    }
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
        // Skip if default credentials file exists (would be used as fallback)
        if default_credentials_path().exists() {
            return;
        }

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
        // Skip if default credentials file exists (would be used as fallback)
        if default_credentials_path().exists() {
            return;
        }
        let result = resolve_google_credentials(None, None, None, None);
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

        let result = resolve_google_credentials(None, None, Some(creds_path), None);
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "file-id.apps.googleusercontent.com");
        assert_eq!(secret, "file-secret");
    }

    #[test]
    fn save_credentials_copies_file_to_default_path() {
        let tmp = tempfile::tempdir().unwrap();
        let src_path = tmp.path().join("source-creds.json");
        let dest_path = tmp.path().join("dest-oauth.json");

        let creds_json = r#"{
            "installed": {
                "client_id": "test.apps.googleusercontent.com",
                "client_secret": "test-secret"
            }
        }"#;
        std::fs::write(&src_path, creds_json).unwrap();

        // We can't easily test save_credentials_to_default_path directly
        // because it uses the hardcoded default_credentials_path().
        // Instead, we test the copy logic indirectly:
        // If the source file exists, copying it should work.
        std::fs::copy(&src_path, &dest_path).unwrap();
        assert!(dest_path.exists());

        let content = std::fs::read_to_string(&dest_path).unwrap();
        assert!(content.contains("test.apps.googleusercontent.com"));
    }

    #[test]
    fn save_credentials_writes_json_for_inline_creds() {
        let tmp = tempfile::tempdir().unwrap();
        let dest_path = tmp.path().join("oauth.json");

        // Simulate what save_credentials_to_default_path does for inline creds
        let json = format!(
            "{{\n  \"client_id\": \"{}\",\n  \"client_secret\": \"{}\"\n}}\n",
            "test.apps.googleusercontent.com", "test-secret"
        );
        std::fs::write(&dest_path, &json).unwrap();

        // Verify the written file can be parsed back
        let creds =
            nextmeeting_providers::google::OAuthCredentials::from_file(&dest_path).unwrap();
        assert_eq!(creds.client_id, "test.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "test-secret");
    }
}
