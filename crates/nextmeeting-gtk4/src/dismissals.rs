use std::collections::HashSet;
use std::path::PathBuf;

use nextmeeting_client::config::ClientConfig;

#[derive(Debug)]
pub struct DismissedEvents {
    event_ids: HashSet<String>,
    path: PathBuf,
}

impl DismissedEvents {
    pub fn load() -> Self {
        let path = ClientConfig::dismissed_events_path();
        let event_ids = std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<Vec<String>>(&content).ok())
            .map(|ids| ids.into_iter().collect())
            .unwrap_or_default();

        Self { event_ids, path }
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut ids: Vec<&String> = self.event_ids.iter().collect();
        ids.sort();
        if let Ok(json) = serde_json::to_string_pretty(&ids) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    pub fn is_dismissed(&self, event_id: &str) -> bool {
        self.event_ids.contains(event_id)
    }

    pub fn dismiss(&mut self, event_id: String) {
        if self.event_ids.insert(event_id) {
            self.save();
        }
    }

    pub fn undismiss(&mut self, event_id: &str) {
        if self.event_ids.remove(event_id) {
            self.save();
        }
    }

    pub fn clear(&mut self) {
        if !self.event_ids.is_empty() {
            self.event_ids.clear();
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dismiss_and_undismiss() {
        let mut dismissed = DismissedEvents {
            event_ids: HashSet::new(),
            path: PathBuf::from("/tmp/nextmeeting-gtk-dismiss-test.json"),
        };
        dismissed.dismiss("evt-1".to_string());
        assert!(dismissed.is_dismissed("evt-1"));
        dismissed.undismiss("evt-1");
        assert!(!dismissed.is_dismissed("evt-1"));
    }
}
