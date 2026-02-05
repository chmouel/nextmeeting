//! Configuration commands.

use crate::config::ClientConfig;
use crate::error::ClientResult;

/// Dump the current configuration to stdout.
pub fn dump(config: &ClientConfig) -> ClientResult<()> {
    let toml_str = toml::to_string_pretty(config).map_err(|e| {
        crate::error::ClientError::Config(format!("failed to serialize config: {}", e))
    })?;
    println!("{}", toml_str);
    Ok(())
}

/// Validate the configuration.
pub fn validate(config: &ClientConfig) -> ClientResult<()> {
    // Validate Google settings if present
    #[cfg(feature = "google")]
    if let Some(ref google) = config.google {
        use nextmeeting_providers::google::OAuthCredentials;

        let credentials = OAuthCredentials::new(&google.client_id, &google.client_secret);
        if let Err(e) = credentials.validate() {
            return Err(crate::error::ClientError::Config(format!(
                "invalid Google credentials: {}",
                e
            )));
        }
    }

    println!("Configuration is valid.");
    Ok(())
}

/// Show the configuration file path.
pub fn path() -> ClientResult<()> {
    let path = ClientConfig::default_path();
    println!("{}", path.display());
    Ok(())
}
