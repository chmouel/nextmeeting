//! Authentication commands.

use std::path::PathBuf;

use tracing::info;

use crate::config::{ClientConfig, GoogleSettings};
use crate::error::ClientResult;

use nextmeeting_providers::CalendarProvider;

/// Run the Google authentication flow.
///
/// Resolves credentials from CLI flags, a `--credentials-file`, or
/// `config.toml`, then runs the OAuth 2.0 PKCE flow.
///
/// When credentials are provided via CLI or `--credentials-file`, they are
/// persisted to `config.toml` so the server can find them.
pub async fn google(
    client_id: Option<String>,
    client_secret: Option<String>,
    credentials_file: Option<PathBuf>,
    domain: Option<String>,
    force: bool,
    config: &ClientConfig,
) -> ClientResult<()> {
    use nextmeeting_providers::google::{GoogleConfig, GoogleProvider, OAuthCredentials};

    // Resolve credentials from CLI args or config.toml
    let (final_client_id, final_client_secret, source) = resolve_google_credentials(
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
        save_credentials_to_config(&final_client_id, &final_client_secret, &source);
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

    // Save credentials to config.toml so the server can find them
    save_credentials_to_config(&final_client_id, &final_client_secret, &source);

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
    /// From config.toml (already persisted)
    Config,
}

/// Saves credentials to `config.toml` under `[google]`.
///
/// Only saves if the credentials came from a transient source (CLI flags or
/// `--credentials-file`). If they're already in config.toml, this is a no-op.
fn save_credentials_to_config(
    client_id: &str,
    client_secret: &str,
    source: &CredentialSource,
) {
    if *source == CredentialSource::Config {
        return;
    }

    let config_path = ClientConfig::default_path();

    // Read existing config or start fresh
    let content = if config_path.exists() {
        std::fs::read_to_string(&config_path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut doc = match content.parse::<toml_edit::DocumentMut>() {
        Ok(d) => d,
        Err(e) => {
            info!("could not parse config.toml for writing: {}", e);
            return;
        }
    };

    // Ensure [google] table exists
    if !doc.contains_key("google") {
        doc["google"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    if let Some(google) = doc["google"].as_table_mut() {
        google["client_id"] = toml_edit::value(client_id);
        google["client_secret"] = toml_edit::value(client_secret);

        // Ensure calendar_ids exists with a default
        if !google.contains_key("calendar_ids") {
            let mut arr = toml_edit::Array::new();
            arr.push("primary");
            google["calendar_ids"] = toml_edit::value(arr);
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            info!(
                "could not create config directory {}: {}",
                parent.display(),
                e
            );
            return;
        }
    }

    match std::fs::write(&config_path, doc.to_string()) {
        Ok(()) => {
            info!("Credentials saved to {}", config_path.display());
            println!("Credentials saved to {}", config_path.display());
        }
        Err(e) => {
            info!(
                "could not save credentials to {}: {}",
                config_path.display(),
                e
            );
        }
    }
}

/// Resolves Google credentials from multiple sources.
///
/// Priority (highest to lowest):
/// 1. CLI `--client-id` + `--client-secret`
/// 2. CLI `--credentials-file` (Google Cloud Console JSON)
/// 3. `config.toml` `[google]` section (client_id + client_secret, with secret resolution)
fn resolve_google_credentials(
    cli_client_id: Option<String>,
    cli_client_secret: Option<String>,
    cli_credentials_file: Option<PathBuf>,
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

    // Priority 3: config.toml [google] section
    if let Some(google) = config_google {
        if google.client_id.is_some() && google.client_secret.is_some() {
            let creds = google.resolve_credentials().map_err(|e| {
                crate::error::ClientError::Config(format!(
                    "failed to resolve Google credentials from config: {}",
                    e
                ))
            })?;
            return Ok((creds.client_id, creds.client_secret, CredentialSource::Config));
        }
    }

    // Handle partial CLI args (only id or only secret provided)
    if cli_client_id.is_some() || cli_client_secret.is_some() {
        return Err(crate::error::ClientError::Config(
            "both --client-id and --client-secret are required when providing credentials directly"
                .to_string(),
        ));
    }

    let config_path = ClientConfig::default_path();
    Err(crate::error::ClientError::Config(format!(
        "Google credentials are required. Provide via:\n  \
         - client_id + client_secret in {}\n  \
         - --client-id and --client-secret flags\n  \
         - --credentials-file flag (path to Google Cloud Console JSON)\n  \
         - GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET env vars",
        config_path.display()
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
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "cli-id.apps.googleusercontent.com");
        assert_eq!(secret, "cli-secret");
        assert_eq!(source, CredentialSource::Cli);
    }

    #[test]
    fn resolve_credentials_from_config() {
        let settings = GoogleSettings {
            client_id: Some("config-id.apps.googleusercontent.com".to_string()),
            client_secret: Some("config-secret".to_string()),
            ..Default::default()
        };
        let result = resolve_google_credentials(None, None, None, Some(&settings));
        assert!(result.is_ok());
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "config-id.apps.googleusercontent.com");
        assert_eq!(secret, "config-secret");
        assert_eq!(source, CredentialSource::Config);
    }

    #[test]
    fn resolve_credentials_cli_overrides_config() {
        let settings = GoogleSettings {
            client_id: Some("config-id.apps.googleusercontent.com".to_string()),
            client_secret: Some("config-secret".to_string()),
            ..Default::default()
        };
        let result = resolve_google_credentials(
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            None,
            Some(&settings),
        );
        assert!(result.is_ok());
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "cli-id.apps.googleusercontent.com");
        assert_eq!(secret, "cli-secret");
        assert_eq!(source, CredentialSource::Cli);
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
        let result =
            resolve_google_credentials(None, Some("secret".to_string()), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_credentials_no_credentials_fails() {
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
        let (id, secret, source) = result.unwrap();
        assert_eq!(id, "file-id.apps.googleusercontent.com");
        assert_eq!(secret, "file-secret");
        assert_eq!(source, CredentialSource::Cli);
    }

    #[test]
    fn save_credentials_skips_when_source_is_config() {
        // This just verifies the no-op path doesn't panic
        save_credentials_to_config("id", "secret", &CredentialSource::Config);
    }

    #[test]
    fn save_credentials_writes_to_config_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "[display]\nno_meeting_text = \"Free\"\n").unwrap();

        // We can't easily test the default path, but we can test the toml_edit logic directly
        let content = std::fs::read_to_string(&config_path).unwrap();
        let mut doc: toml_edit::DocumentMut = content.parse().unwrap();

        if !doc.contains_key("google") {
            doc["google"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if let Some(google) = doc["google"].as_table_mut() {
            google["client_id"] = toml_edit::value("test.apps.googleusercontent.com");
            google["client_secret"] = toml_edit::value("test-secret");
        }

        std::fs::write(&config_path, doc.to_string()).unwrap();

        // Verify it parses back correctly
        let reloaded: ClientConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let google = reloaded.google.unwrap();
        assert_eq!(
            google.client_id,
            Some("test.apps.googleusercontent.com".to_string())
        );
        assert_eq!(google.client_secret, Some("test-secret".to_string()));

        // Verify existing config is preserved
        assert_eq!(reloaded.display.no_meeting_text, "Free");
    }
}
