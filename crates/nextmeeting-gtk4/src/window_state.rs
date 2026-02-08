use std::path::PathBuf;

use nextmeeting_client::config::ClientConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowStateData {
    #[serde(default)]
    sidebar_collapsed: bool,
}

impl Default for WindowStateData {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false, // Expanded by default
        }
    }
}

#[derive(Debug)]
pub struct WindowState {
    data: WindowStateData,
    path: PathBuf,
}

impl WindowState {
    pub fn load() -> Self {
        let path = ClientConfig::default_data_dir().join("window-state.json");
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        Self { data, path }
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    pub fn is_sidebar_collapsed(&self) -> bool {
        self.data.sidebar_collapsed
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool) {
        if self.data.sidebar_collapsed != collapsed {
            self.data.sidebar_collapsed = collapsed;
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_expanded() {
        let data = WindowStateData::default();
        assert!(!data.sidebar_collapsed);
    }

    #[test]
    fn toggle_state() {
        let mut state = WindowState {
            data: WindowStateData::default(),
            path: PathBuf::from("/tmp/nextmeeting-gtk-window-state-test.json"),
        };
        assert!(!state.is_sidebar_collapsed());
        state.set_sidebar_collapsed(true);
        assert!(state.is_sidebar_collapsed());
        state.set_sidebar_collapsed(false);
        assert!(!state.is_sidebar_collapsed());
    }
}
