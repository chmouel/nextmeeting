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
}
