//! Authentication commands.

use std::path::PathBuf;

use tracing::info;

use crate::config::ClientConfig;
use crate::error::ClientResult;

use nextmeeting_providers::CalendarProvider;

/// Prints a step-by-step Google Calendar setup guide for first-time users.
pub fn print_google_setup_guide() {
    println!("Google Calendar setup guide");
    println!();
    println!("1. Open Google Cloud Console and create or select a project.");
    println!("2. Enable the Google Calendar API for that project.");
    println!("3. Create an OAuth client ID of type 'Desktop app'.");
    println!("4. Download the OAuth credentials JSON file.");
    println!("5. Run one of these commands:");
    println!();
    println!("   nextmeeting auth google --credentials-file /path/to/client_secret_<id>.json");
    println!(
        "   nextmeeting auth google --account work --credentials-file /path/to/client_secret_<id>.json"
    );
    println!();
    println!(
        "The credentials file path will be saved in {}.",
        ClientConfig::default_path().display()
    );
    println!("Tokens are stored separately in ~/.local/share/nextmeeting/.");
    println!();
    println!("After authentication, run `nextmeeting` to show your next meeting.");
}

/// Run the Google authentication flow.
///
/// Resolves credentials from CLI flags, a `--credentials-file`, or
/// `config.toml`, then runs the OAuth 2.0 PKCE flow.
///
/// When credentials are provided via CLI or `--credentials-file`, they are
/// persisted to `config.toml` so the server can find them.
pub async fn google(
    account: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    credentials_file: Option<PathBuf>,
    domain: Option<String>,
    force: bool,
    config: &ClientConfig,
) -> ClientResult<()> {
    use nextmeeting_providers::google::{GoogleConfig, GoogleProvider, OAuthCredentials};

    // Resolve the target account
    let resolved = resolve_target_account(
        account.as_deref(),
        client_id,
        client_secret,
        credentials_file,
        domain,
        config,
    )?;

    // Build provider configuration
    let credentials =
        OAuthCredentials::new(&resolved.final_client_id, &resolved.final_client_secret);
    credentials.validate().map_err(|e| {
        crate::error::ClientError::Config(format!("invalid Google credentials: {}", e))
    })?;

    let mut google_config =
        GoogleConfig::new(credentials).with_account_name(&resolved.account_name);

    if let Some(ref d) = resolved.final_domain {
        google_config = google_config.with_domain(d);
    }

    // Apply token path if set
    if let Some(ref path) = resolved.token_path {
        google_config = google_config.with_token_path(path);
    }

    // Create the provider
    let provider = GoogleProvider::new(google_config)?;

    // Check if already authenticated
    if provider.is_authenticated() && !force {
        save_credentials_to_config(&resolved);
        println!(
            "Already authenticated with Google Calendar (account: {}).",
            resolved.account_name
        );
        println!("Use --force to re-authenticate.");
        return Ok(());
    }

    // Perform authentication
    println!(
        "Starting Google Calendar authentication for account '{}'...",
        resolved.account_name
    );
    println!();
    println!("A browser window will open for you to authorize access.");
    println!("If the browser doesn't open, check the terminal for a URL to copy.");
    println!();

    provider.authenticate().await?;

    // Save credentials to config.toml so the server can find them
    save_credentials_to_config(&resolved);

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

/// Resolved account information for the auth flow.
#[derive(Debug)]
struct ResolvedAccount {
    account_name: String,
    final_client_id: String,
    final_client_secret: String,
    final_domain: Option<String>,
    token_path: Option<PathBuf>,
    source: CredentialSource,
    persistence: Option<CredentialPersistence>,
}

/// How Google credentials should be persisted in config.toml.
#[derive(Debug, Clone, PartialEq)]
enum CredentialPersistence {
    /// Save a reference to the downloaded OAuth credentials JSON file.
    CredentialsFile(PathBuf),
    /// Save inline client ID and client secret values.
    Inline {
        client_id: String,
        client_secret: String,
    },
}

/// Resolved credentials coming from CLI flags.
#[derive(Debug)]
struct ResolvedCliCredentials {
    client_id: String,
    client_secret: String,
    persistence: CredentialPersistence,
}

/// Resolves the target account, credentials, and domain for the auth flow.
///
/// Account resolution logic:
/// - If CLI credentials are provided, `--account` name is required
/// - If `--account <name>` given, look up that account in config
/// - If no `--account`: use single account if only one exists, error with list if multiple
fn resolve_target_account(
    cli_account: Option<&str>,
    cli_client_id: Option<String>,
    cli_client_secret: Option<String>,
    cli_credentials_file: Option<PathBuf>,
    cli_domain: Option<String>,
    config: &ClientConfig,
) -> ClientResult<ResolvedAccount> {
    let has_cli_creds =
        cli_client_id.is_some() || cli_client_secret.is_some() || cli_credentials_file.is_some();

    // Priority 1: CLI credentials (default to "default" if --account not specified)
    if has_cli_creds {
        let account_name = cli_account.unwrap_or("default");

        let resolved_cli =
            resolve_cli_credentials(cli_client_id, cli_client_secret, cli_credentials_file)?;

        return Ok(ResolvedAccount {
            account_name: account_name.to_string(),
            final_client_id: resolved_cli.client_id,
            final_client_secret: resolved_cli.client_secret,
            final_domain: cli_domain,
            token_path: None,
            source: CredentialSource::Cli,
            persistence: Some(resolved_cli.persistence),
        });
    }

    // No CLI credentials — resolve from config
    let accounts = config
        .google
        .as_ref()
        .map(|g| &g.accounts[..])
        .unwrap_or(&[]);

    let target_account = match cli_account {
        Some(name) => {
            // Look up specific account
            accounts
                .iter()
                .find(|a| a.name == name)
                .ok_or_else(|| {
                    let available: Vec<&str> = accounts.iter().map(|a| a.name.as_str()).collect();
                    if available.is_empty() {
                        crate::error::ClientError::Config(format!(
                            "account '{}' not found in config. No accounts are configured.\n  \
                             Run `nextmeeting auth google --guide` for setup help.\n  \
                             Quick start:\n  \
                             nextmeeting auth google --account {} --credentials-file /path/to/client_secret_<id>.json",
                            name,
                            name
                        ))
                    } else {
                        crate::error::ClientError::Config(format!(
                            "account '{}' not found in config. Available accounts: {}",
                            name,
                            available.join(", ")
                        ))
                    }
                })?
        }
        None => {
            // Auto-select if only one account
            match accounts.len() {
                0 => {
                    let config_path = ClientConfig::default_path();
                    return Err(crate::error::ClientError::Config(format!(
                        "no Google accounts configured.\n  \
                         Run `nextmeeting auth google --guide` for the full setup.\n  \
                         Quick start:\n  \
                         - nextmeeting auth google --credentials-file /path/to/client_secret_<id>.json\n  \
                         - nextmeeting auth google --account work --credentials-file /path/to/client_secret_<id>.json\n  \
                         - Or add a [[google.accounts]] entry in {}",
                        config_path.display()
                    )));
                }
                1 => &accounts[0],
                _ => {
                    let names: Vec<&str> = accounts.iter().map(|a| a.name.as_str()).collect();
                    return Err(crate::error::ClientError::Config(format!(
                        "multiple Google accounts configured. Use --account to specify which one:\n  {}",
                        names
                            .iter()
                            .map(|n| format!("nextmeeting auth google --account {}", n))
                            .collect::<Vec<_>>()
                            .join("\n  ")
                    )));
                }
            }
        }
    };

    let creds = target_account.resolve_credentials().map_err(|e| {
        crate::error::ClientError::Config(format!(
            "failed to resolve credentials for account '{}': {}",
            target_account.name, e
        ))
    })?;

    let final_domain = cli_domain.or_else(|| target_account.domain.clone());

    Ok(ResolvedAccount {
        account_name: target_account.name.clone(),
        final_client_id: creds.client_id,
        final_client_secret: creds.client_secret,
        final_domain,
        token_path: target_account.token_path.clone(),
        source: CredentialSource::Config,
        persistence: None,
    })
}

/// Resolves credentials from CLI flags (--client-id/--client-secret or --credentials-file).
fn resolve_cli_credentials(
    cli_client_id: Option<String>,
    cli_client_secret: Option<String>,
    cli_credentials_file: Option<PathBuf>,
) -> ClientResult<ResolvedCliCredentials> {
    use nextmeeting_providers::google::OAuthCredentials;

    // CLI client_id + client_secret
    if let (Some(id), Some(secret)) = (&cli_client_id, &cli_client_secret) {
        return Ok(ResolvedCliCredentials {
            client_id: id.clone(),
            client_secret: secret.clone(),
            persistence: CredentialPersistence::Inline {
                client_id: id.clone(),
                client_secret: secret.clone(),
            },
        });
    }

    // CLI credentials file
    if let Some(ref path) = cli_credentials_file {
        let creds = OAuthCredentials::from_file(path).map_err(|e| {
            crate::error::ClientError::Config(format!(
                "failed to load credentials from {}: {}",
                path.display(),
                e
            ))
        })?;
        return Ok(ResolvedCliCredentials {
            client_id: creds.client_id,
            client_secret: creds.client_secret,
            persistence: CredentialPersistence::CredentialsFile(path.clone()),
        });
    }

    // Partial CLI args
    if cli_client_id.is_some() || cli_client_secret.is_some() {
        return Err(crate::error::ClientError::Config(
            "both --client-id and --client-secret are required when providing credentials directly"
                .to_string(),
        ));
    }

    Err(crate::error::ClientError::Config(
        "no credentials provided".to_string(),
    ))
}

/// Saves credentials to `config.toml` under `[[google.accounts]]`.
///
/// Only saves if the credentials came from a transient source (CLI flags or
/// `--credentials-file`). If they're already in config.toml, this is a no-op.
fn save_credentials_to_config(resolved: &ResolvedAccount) {
    if resolved.source == CredentialSource::Config {
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

    // Find or create the account entry in [[google.accounts]]
    let google = doc["google"].as_table_mut().unwrap();

    if !google.contains_key("accounts") {
        google["accounts"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
    }

    let accounts = google["accounts"].as_array_of_tables_mut().unwrap();

    // Check if account already exists
    let mut found = false;
    for table in accounts.iter_mut() {
        if let Some(name) = table.get("name").and_then(|v| v.as_str())
            && name == resolved.account_name
        {
            // Update existing account
            apply_credentials(table, resolved.persistence.as_ref().unwrap());
            if let Some(ref domain) = resolved.final_domain {
                table["domain"] = toml_edit::value(domain.as_str());
            }
            if !table.contains_key("calendar_ids") {
                let mut arr = toml_edit::Array::new();
                arr.push("primary");
                table["calendar_ids"] = toml_edit::value(arr);
            }
            found = true;
            break;
        }
    }

    if !found {
        // Create new account entry
        let mut table = toml_edit::Table::new();
        table["name"] = toml_edit::value(&resolved.account_name);
        apply_credentials(&mut table, resolved.persistence.as_ref().unwrap());
        if let Some(ref domain) = resolved.final_domain {
            table["domain"] = toml_edit::value(domain.as_str());
        }
        let mut arr = toml_edit::Array::new();
        arr.push("primary");
        table["calendar_ids"] = toml_edit::value(arr);
        accounts.push(table);
    }

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        info!(
            "could not create config directory {}: {}",
            parent.display(),
            e
        );
        return;
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

fn apply_credentials(table: &mut toml_edit::Table, persistence: &CredentialPersistence) {
    match persistence {
        CredentialPersistence::CredentialsFile(path) => {
            table.remove("client_id");
            table.remove("client_secret");
            table["credentials_file"] = toml_edit::value(path.to_string_lossy().to_string());
        }
        CredentialPersistence::Inline {
            client_id,
            client_secret,
        } => {
            table.remove("credentials_file");
            table["client_id"] = toml_edit::value(client_id);
            table["client_secret"] = toml_edit::value(client_secret);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GoogleAccountSettings, GoogleSettings};

    fn test_account(name: &str) -> GoogleAccountSettings {
        GoogleAccountSettings {
            name: name.to_string(),
            client_id: Some(format!("{}-id.apps.googleusercontent.com", name)),
            client_secret: Some(format!("{}-secret", name)),
            credentials_file: None,
            domain: None,
            calendar_ids: vec!["primary".to_string()],
            token_path: None,
        }
    }

    fn config_with_accounts(accounts: Vec<GoogleAccountSettings>) -> ClientConfig {
        ClientConfig {
            google: Some(GoogleSettings { accounts }),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_credentials_from_cli_with_account() {
        let config = ClientConfig::default();
        let result = resolve_target_account(
            Some("work"),
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            None,
            None,
            &config,
        );
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.account_name, "work");
        assert_eq!(
            resolved.final_client_id,
            "cli-id.apps.googleusercontent.com"
        );
        assert_eq!(resolved.final_client_secret, "cli-secret");
        assert_eq!(resolved.source, CredentialSource::Cli);
        assert_eq!(
            resolved.persistence,
            Some(CredentialPersistence::Inline {
                client_id: "cli-id.apps.googleusercontent.com".to_string(),
                client_secret: "cli-secret".to_string(),
            })
        );
    }

    #[test]
    fn resolve_cli_credentials_defaults_to_default_account() {
        let config = ClientConfig::default();
        let result = resolve_target_account(
            None, // no --account
            Some("cli-id.apps.googleusercontent.com".to_string()),
            Some("cli-secret".to_string()),
            None,
            None,
            &config,
        );
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.account_name, "default");
        assert_eq!(
            resolved.final_client_id,
            "cli-id.apps.googleusercontent.com"
        );
        assert_eq!(resolved.final_client_secret, "cli-secret");
        assert_eq!(resolved.source, CredentialSource::Cli);
    }

    #[test]
    fn resolve_cli_credentials_defaults_to_default_even_with_existing_accounts() {
        // Even when other accounts exist, CLI credentials without --account should use "default"
        let config = config_with_accounts(vec![test_account("work"), test_account("personal")]);
        let result = resolve_target_account(
            None, // no --account
            Some("new-id.apps.googleusercontent.com".to_string()),
            Some("new-secret".to_string()),
            None,
            None,
            &config,
        );
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.account_name, "default");
        assert_eq!(
            resolved.final_client_id,
            "new-id.apps.googleusercontent.com"
        );
        assert_eq!(resolved.final_client_secret, "new-secret");
        assert_eq!(resolved.source, CredentialSource::Cli);
    }

    #[test]
    fn resolve_from_config_single_account() {
        let config = config_with_accounts(vec![test_account("work")]);
        let result = resolve_target_account(None, None, None, None, None, &config);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.account_name, "work");
        assert_eq!(
            resolved.final_client_id,
            "work-id.apps.googleusercontent.com"
        );
        assert_eq!(resolved.source, CredentialSource::Config);
    }

    #[test]
    fn resolve_from_config_multiple_accounts_requires_account() {
        let config = config_with_accounts(vec![test_account("work"), test_account("personal")]);
        let result = resolve_target_account(None, None, None, None, None, &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--account"));
    }

    #[test]
    fn resolve_from_config_named_account() {
        let config = config_with_accounts(vec![test_account("work"), test_account("personal")]);
        let result = resolve_target_account(Some("personal"), None, None, None, None, &config);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.account_name, "personal");
        assert_eq!(
            resolved.final_client_id,
            "personal-id.apps.googleusercontent.com"
        );
    }

    #[test]
    fn resolve_from_config_nonexistent_account() {
        let config = config_with_accounts(vec![test_account("work")]);
        let result = resolve_target_account(Some("missing"), None, None, None, None, &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn resolve_no_accounts_fails() {
        let config = ClientConfig::default();
        let result = resolve_target_account(None, None, None, None, None, &config);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_credentials_partial_cli_fails() {
        let config = ClientConfig::default();
        // Only client_id without client_secret should fail
        let result = resolve_target_account(
            Some("test"),
            Some("id.apps.googleusercontent.com".to_string()),
            None,
            None,
            None,
            &config,
        );
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

        let config = ClientConfig::default();
        let result = resolve_target_account(
            Some("work"),
            None,
            None,
            Some(creds_path.clone()),
            None,
            &config,
        );
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.account_name, "work");
        assert_eq!(
            resolved.final_client_id,
            "file-id.apps.googleusercontent.com"
        );
        assert_eq!(resolved.final_client_secret, "file-secret");
        assert_eq!(resolved.source, CredentialSource::Cli);
        assert_eq!(
            resolved.persistence,
            Some(CredentialPersistence::CredentialsFile(creds_path))
        );
    }

    #[test]
    fn save_credentials_skips_when_source_is_config() {
        let resolved = ResolvedAccount {
            account_name: "test".to_string(),
            final_client_id: "id".to_string(),
            final_client_secret: "secret".to_string(),
            final_domain: None,
            token_path: None,
            source: CredentialSource::Config,
            persistence: None,
        };
        // This just verifies the no-op path doesn't panic
        save_credentials_to_config(&resolved);
    }

    #[test]
    fn save_credentials_writes_accounts_format() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "[display]\nno_meeting_text = \"Free\"\n").unwrap();

        // Test the toml_edit logic for [[google.accounts]] format
        let content = std::fs::read_to_string(&config_path).unwrap();
        let mut doc: toml_edit::DocumentMut = content.parse().unwrap();

        if !doc.contains_key("google") {
            doc["google"] = toml_edit::Item::Table(toml_edit::Table::new());
        }

        let google = doc["google"].as_table_mut().unwrap();
        if !google.contains_key("accounts") {
            google["accounts"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
        }

        let accounts = google["accounts"].as_array_of_tables_mut().unwrap();
        let mut table = toml_edit::Table::new();
        table["name"] = toml_edit::value("work");
        table["client_id"] = toml_edit::value("test.apps.googleusercontent.com");
        table["client_secret"] = toml_edit::value("test-secret");
        accounts.push(table);

        std::fs::write(&config_path, doc.to_string()).unwrap();

        // Verify it parses back correctly
        let reloaded: ClientConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let google = reloaded.google.unwrap();
        assert_eq!(google.accounts.len(), 1);
        assert_eq!(google.accounts[0].name, "work");
        assert_eq!(
            google.accounts[0].client_id,
            Some("test.apps.googleusercontent.com".to_string())
        );
        assert_eq!(
            google.accounts[0].client_secret,
            Some("test-secret".to_string())
        );

        // Verify existing config is preserved
        assert_eq!(reloaded.display.no_meeting_text, "Free");
    }
}
