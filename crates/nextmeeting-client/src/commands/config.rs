//! Configuration commands.

use crate::config::{AuthConfig, ClientConfig};
use crate::error::ClientResult;

/// Dump the current configuration to stdout.
pub fn dump(config: &ClientConfig) -> ClientResult<()> {
    let toml_str = toml::to_string_pretty(config).map_err(|e| {
        crate::error::ClientError::Config(format!("failed to serialize config: {}", e))
    })?;
    println!("# config.toml");
    println!("{}", toml_str);

    // Also show auth.yaml status
    let auth_path = AuthConfig::default_path();
    if auth_path.exists() {
        println!(
            "# auth.yaml ({}) — credentials present",
            auth_path.display()
        );
    } else {
        println!("# auth.yaml ({}) — not found", auth_path.display());
    }

    Ok(())
}

/// Validate the configuration.
pub fn validate(config: &ClientConfig) -> ClientResult<()> {
    // Validate Google settings if present
    #[cfg(feature = "google")]
    if let Some(ref google) = config.google {
        // Validate that calendar_ids is not empty
        if google.calendar_ids.is_empty() {
            return Err(crate::error::ClientError::Config(
                "Google calendar_ids must not be empty".to_string(),
            ));
        }

        // Validate credentials_file path if specified
        if let Some(ref path) = google.credentials_file {
            if !path.exists() {
                return Err(crate::error::ClientError::Config(format!(
                    "credentials_file does not exist: {}",
                    path.display()
                )));
            }
        }
    }

    // Validate auth.yaml if it exists
    let auth_path = AuthConfig::default_path();
    if auth_path.exists() {
        AuthConfig::load_from(&auth_path)
            .map_err(|e| crate::error::ClientError::Config(format!("invalid auth.yaml: {}", e)))?;
        println!("auth.yaml is valid.");
    }

    println!("Configuration is valid.");
    Ok(())
}

/// Show the configuration file path.
pub fn path() -> ClientResult<()> {
    let config_path = ClientConfig::default_path();
    let auth_path = AuthConfig::default_path();
    println!("config: {}", config_path.display());
    println!("auth:   {}", auth_path.display());
    Ok(())
}
