use std::collections::HashSet;
use std::path::PathBuf;

use nextmeeting_client::config::ClientConfig;

/// Tracks dismissed event IDs so they can be hidden from the dashboard.
#[derive(Debug)]
pub struct DismissedEvents {
    event_ids: HashSet<String>,
    path: PathBuf,
}

impl DismissedEvents {
    /// Loads dismissed events from disk, falling back to an empty set.
    pub fn load() -> Self {
        let path = ClientConfig::dismissed_events_path();
        let event_ids = std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<Vec<String>>(&content).ok())
            .map(|ids| ids.into_iter().collect())
            .unwrap_or_default();

        Self { event_ids, path }
    }

    /// Persists the current set to disk.
    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let ids: Vec<&String> = self.event_ids.iter().collect();
        if let Ok(json) = serde_json::to_string_pretty(&ids) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    /// Returns true if the given event ID has been dismissed.
    pub fn is_dismissed(&self, event_id: &str) -> bool {
        self.event_ids.contains(event_id)
    }

    /// Dismisses an event by ID.
    pub fn dismiss(&mut self, event_id: String) {
        if self.event_ids.insert(event_id) {
            self.save();
        }
    }

    /// Undismisses an event by ID.
    pub fn undismiss(&mut self, event_id: &str) {
        if self.event_ids.remove(event_id) {
            self.save();
        }
    }

    /// Clears all dismissed events.
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
    fn dismiss_and_check() {
        let mut dismissed = DismissedEvents {
            event_ids: HashSet::new(),
            path: PathBuf::from("/tmp/nextmeeting-test-dismiss.json"),
        };

        assert!(!dismissed.is_dismissed("evt-1"));
        dismissed.dismiss("evt-1".to_string());
        assert!(dismissed.is_dismissed("evt-1"));
    }

    #[test]
    fn undismiss() {
        let mut dismissed = DismissedEvents {
            event_ids: HashSet::new(),
            path: PathBuf::from("/tmp/nextmeeting-test-undismiss.json"),
        };

        dismissed.dismiss("evt-1".to_string());
        assert!(dismissed.is_dismissed("evt-1"));
        dismissed.undismiss("evt-1");
        assert!(!dismissed.is_dismissed("evt-1"));
    }

    #[test]
    fn clear_all() {
        let mut dismissed = DismissedEvents {
            event_ids: HashSet::new(),
            path: PathBuf::from("/tmp/nextmeeting-test-clear.json"),
        };

        dismissed.dismiss("evt-1".to_string());
        dismissed.dismiss("evt-2".to_string());
        dismissed.clear();
        assert!(!dismissed.is_dismissed("evt-1"));
        assert!(!dismissed.is_dismissed("evt-2"));
    }
}
