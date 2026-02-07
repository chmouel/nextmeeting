//! Configuration commands.

use crate::config::ClientConfig;
use crate::error::ClientResult;

/// Dump the current configuration to stdout.
pub fn dump(config: &ClientConfig) -> ClientResult<()> {
    let toml_str = toml::to_string_pretty(config).map_err(|e| {
        crate::error::ClientError::Config(format!("failed to serialize config: {}", e))
    })?;
    println!("# config.toml ({})", ClientConfig::default_path().display());
    println!("{}", toml_str);

    Ok(())
}

/// Validate the configuration.
pub fn validate(config: &ClientConfig) -> ClientResult<()> {
    // Validate Google settings if present
    #[cfg(feature = "google")]
    if let Some(ref google) = config.google {
        // Validate account structure (duplicate names, invalid characters, etc.)
        google.validate().map_err(|e| {
            crate::error::ClientError::Config(format!("invalid Google configuration: {}", e))
        })?;

        for account in &google.accounts {
            // Validate that calendar_ids is not empty
            if account.calendar_ids.is_empty() {
                return Err(crate::error::ClientError::Config(format!(
                    "Google account '{}': calendar_ids must not be empty",
                    account.name
                )));
            }

            // Validate credentials if present
            if account.client_id.is_some() || account.client_secret.is_some() {
                account.resolve_credentials().map_err(|e| {
                    crate::error::ClientError::Config(format!(
                        "invalid Google credentials for account '{}': {}",
                        account.name, e
                    ))
                })?;
                println!("Google account '{}': credentials are valid.", account.name);
            }
        }
    }

    println!("Configuration is valid.");
    Ok(())
}

/// Show the configuration file path.
pub fn path() -> ClientResult<()> {
    let config_path = ClientConfig::default_path();
    println!("config: {}", config_path.display());
    Ok(())
}
