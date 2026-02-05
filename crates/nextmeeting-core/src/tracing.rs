//! Tracing setup for nextmeeting
//!
//! Provides unified logging and tracing configuration for all crates.
//!
//! # Usage
//!
//! For CLI applications:
//! ```ignore
//! use nextmeeting_core::tracing::{init_tracing, TracingConfig};
//!
//! init_tracing(TracingConfig::default()).expect("failed to initialize tracing");
//! ```
//!
//! For the daemon/server:
//! ```ignore
//! use nextmeeting_core::tracing::{init_tracing, TracingConfig, OutputFormat};
//!
//! init_tracing(TracingConfig {
//!     output_format: OutputFormat::Json,
//!     ..Default::default()
//! }).expect("failed to initialize tracing");
//! ```

use thiserror::Error;
use tracing::Level;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, format::FmtSpan},
    prelude::*,
};

/// Errors that can occur during tracing initialization
#[derive(Debug, Error)]
pub enum TracingError {
    /// Failed to set global subscriber
    #[error("failed to set global tracing subscriber: {0}")]
    SetGlobalSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),

    /// Failed to parse env filter directive
    #[error("failed to parse env filter: {0}")]
    EnvFilter(#[from] tracing_subscriber::filter::ParseError),
}

/// Output format for tracing logs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TracingOutputFormat {
    /// Human-readable pretty format (default)
    #[default]
    Pretty,
    /// Compact single-line format
    Compact,
    /// JSON format (useful for structured logging in daemon mode)
    Json,
}

/// Configuration for tracing initialization
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// The default log level when RUST_LOG is not set
    pub default_level: Level,
    /// Output format for log messages
    pub output_format: TracingOutputFormat,
    /// Whether to include file/line information in logs
    pub include_location: bool,
    /// Whether to include target (module path) in logs
    pub include_target: bool,
    /// Whether to include timestamps
    pub include_timestamp: bool,
    /// Whether to include span events (enter/exit)
    pub include_span_events: bool,
    /// Custom env filter directive (overrides default_level if set)
    pub env_filter: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            default_level: Level::INFO,
            output_format: TracingOutputFormat::Pretty,
            include_location: false,
            include_target: true,
            include_timestamp: true,
            include_span_events: false,
            env_filter: None,
        }
    }
}

impl TracingConfig {
    /// Create a config suitable for CLI usage with debug mode
    #[must_use]
    pub fn cli_debug() -> Self {
        Self {
            default_level: Level::DEBUG,
            output_format: TracingOutputFormat::Compact,
            include_location: true,
            include_target: true,
            include_timestamp: false,
            include_span_events: false,
            env_filter: None,
        }
    }

    /// Create a config suitable for daemon/server usage
    #[must_use]
    pub fn daemon() -> Self {
        Self {
            default_level: Level::INFO,
            output_format: TracingOutputFormat::Json,
            include_location: true,
            include_target: true,
            include_timestamp: true,
            include_span_events: true,
            env_filter: None,
        }
    }

    /// Set the default log level
    #[must_use]
    pub fn with_level(mut self, level: Level) -> Self {
        self.default_level = level;
        self
    }

    /// Set the output format
    #[must_use]
    pub fn with_format(mut self, format: TracingOutputFormat) -> Self {
        self.output_format = format;
        self
    }

    /// Set a custom env filter directive
    #[must_use]
    pub fn with_env_filter(mut self, filter: impl Into<String>) -> Self {
        self.env_filter = Some(filter.into());
        self
    }
}

/// Initialize tracing with the given configuration.
///
/// This should be called once at the start of the application.
/// The `RUST_LOG` environment variable can be used to override the default level.
///
/// # Errors
///
/// Returns an error if the global subscriber has already been set or if
/// the env filter directive is invalid.
///
/// # Example
///
/// ```ignore
/// use nextmeeting_core::tracing::{init_tracing, TracingConfig};
///
/// // Basic initialization with defaults
/// init_tracing(TracingConfig::default())?;
///
/// // CLI with debug mode
/// init_tracing(TracingConfig::cli_debug())?;
///
/// // Daemon with JSON output
/// init_tracing(TracingConfig::daemon())?;
/// ```
pub fn init_tracing(config: TracingConfig) -> Result<(), TracingError> {
    // Build the env filter
    let env_filter = if let Some(ref filter) = config.env_filter {
        EnvFilter::try_new(filter)?
    } else {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("nextmeeting={}", config.default_level)))
    };

    // Determine span events
    let span_events = if config.include_span_events {
        FmtSpan::NEW | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    match config.output_format {
        TracingOutputFormat::Pretty => {
            let subscriber = tracing_subscriber::registry().with(env_filter).with(
                fmt::layer()
                    .pretty()
                    .with_file(config.include_location)
                    .with_line_number(config.include_location)
                    .with_target(config.include_target)
                    .with_span_events(span_events),
            );
            tracing::subscriber::set_global_default(subscriber)?;
        }
        TracingOutputFormat::Compact => {
            let layer = fmt::layer()
                .compact()
                .with_file(config.include_location)
                .with_line_number(config.include_location)
                .with_target(config.include_target)
                .with_span_events(span_events);

            let layer = if config.include_timestamp {
                layer.boxed()
            } else {
                layer.without_time().boxed()
            };

            let subscriber = tracing_subscriber::registry().with(env_filter).with(layer);
            tracing::subscriber::set_global_default(subscriber)?;
        }
        TracingOutputFormat::Json => {
            let subscriber = tracing_subscriber::registry().with(env_filter).with(
                fmt::layer()
                    .json()
                    .with_file(config.include_location)
                    .with_line_number(config.include_location)
                    .with_target(config.include_target)
                    .with_span_events(span_events),
            );
            tracing::subscriber::set_global_default(subscriber)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TracingConfig::default();
        assert_eq!(config.default_level, Level::INFO);
        assert_eq!(config.output_format, TracingOutputFormat::Pretty);
        assert!(!config.include_location);
        assert!(config.include_target);
        assert!(config.include_timestamp);
        assert!(!config.include_span_events);
        assert!(config.env_filter.is_none());
    }

    #[test]
    fn test_cli_debug_config() {
        let config = TracingConfig::cli_debug();
        assert_eq!(config.default_level, Level::DEBUG);
        assert_eq!(config.output_format, TracingOutputFormat::Compact);
        assert!(config.include_location);
    }

    #[test]
    fn test_daemon_config() {
        let config = TracingConfig::daemon();
        assert_eq!(config.default_level, Level::INFO);
        assert_eq!(config.output_format, TracingOutputFormat::Json);
        assert!(config.include_span_events);
    }

    #[test]
    fn test_builder_methods() {
        let config = TracingConfig::default()
            .with_level(Level::WARN)
            .with_format(TracingOutputFormat::Json)
            .with_env_filter("nextmeeting=trace");

        assert_eq!(config.default_level, Level::WARN);
        assert_eq!(config.output_format, TracingOutputFormat::Json);
        assert_eq!(config.env_filter, Some("nextmeeting=trace".to_string()));
    }
}
