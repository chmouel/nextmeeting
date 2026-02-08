use nextmeeting_client::config::ClientConfig;

#[derive(Debug, Clone)]
pub struct GtkConfig {
    pub client: ClientConfig,
}

impl GtkConfig {
    pub fn load() -> Self {
        Self {
            client: ClientConfig::load().unwrap_or_default(),
        }
    }

    /// Returns the configured snooze duration in minutes (default: 10).
    pub fn snooze_minutes(&self) -> u32 {
        self.client.notifications.snooze_minutes.unwrap_or(10)
    }
}
