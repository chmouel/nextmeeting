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

    config.notifications.validate_end_warning().map_err(|e| {
        crate::error::ClientError::Config(format!("invalid notifications configuration: {}", e))
    })?;

    println!("Configuration is valid.");
    Ok(())
}

/// Show the configuration file path.
pub fn path() -> ClientResult<()> {
    let config_path = ClientConfig::default_path();
    println!("config: {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::config::ClientConfig;

    #[test]
    fn validate_rejects_enabled_end_warning_without_threshold() {
        let mut config = ClientConfig::default();
        config.notifications.end_warning_enabled = true;
        let err = validate(&config).unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid notifications configuration")
        );
    }

    #[test]
    fn validate_accepts_enabled_end_warning_with_threshold() {
        let mut config = ClientConfig::default();
        config.notifications.end_warning_enabled = true;
        config.notifications.end_warning_minutes_before = Some(5);
        assert!(validate(&config).is_ok());
    }
}
