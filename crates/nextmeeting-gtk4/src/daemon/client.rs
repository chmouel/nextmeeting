use std::time::Duration;

use nextmeeting_client::config::ClientConfig;
use nextmeeting_client::socket::SocketClient;
use nextmeeting_core::MeetingView;
use nextmeeting_protocol::{EventMutationAction, MeetingsFilter, Request, Response};

#[derive(Debug, Clone)]
pub struct DaemonClient {
    socket: std::path::PathBuf,
    timeout_secs: u64,
    filter: MeetingsFilter,
}

impl DaemonClient {
    pub fn from_config(config: &ClientConfig) -> Self {
        let socket = config
            .server
            .socket_path
            .clone()
            .unwrap_or_else(nextmeeting_server::default_socket_path);
        Self {
            socket,
            timeout_secs: config.server.timeout.max(1),
            filter: build_filter(config),
        }
    }

    fn socket_client(&self) -> SocketClient {
        SocketClient::new(self.socket.clone(), Duration::from_secs(self.timeout_secs))
    }

    pub async fn get_meetings(&self) -> Result<Vec<MeetingView>, String> {
        let request = if filter_is_empty(&self.filter) {
            Request::get_meetings()
        } else {
            Request::get_meetings_with_filter(self.filter.clone())
        };
        let response = self
            .socket_client()
            .send(request)
            .await
            .map_err(|e| e.to_string())?;
        match response {
            Response::Meetings { meetings } => Ok(meetings),
            Response::Error { error } => Err(error.message),
            other => Err(format!("unexpected response: {other:?}")),
        }
    }

    pub async fn refresh(&self) -> Result<(), String> {
        let response = self
            .socket_client()
            .send(Request::refresh(true))
            .await
            .map_err(|e| e.to_string())?;
        match response {
            Response::Ok => Ok(()),
            Response::Error { error } => Err(error.message),
            other => Err(format!("unexpected response: {other:?}")),
        }
    }

    pub async fn snooze(&self, minutes: u32) -> Result<(), String> {
        let response = self
            .socket_client()
            .send(Request::snooze(minutes))
            .await
            .map_err(|e| e.to_string())?;
        match response {
            Response::Ok => Ok(()),
            Response::Error { error } => Err(error.message),
            other => Err(format!("unexpected response: {other:?}")),
        }
    }

    pub async fn mutate_event(
        &self,
        provider_name: &str,
        calendar_id: &str,
        event_id: &str,
        action: EventMutationAction,
    ) -> Result<(), String> {
        let response = self
            .socket_client()
            .send(Request::mutate_event(
                provider_name,
                calendar_id,
                event_id,
                action,
            ))
            .await
            .map_err(|e| e.to_string())?;
        match response {
            Response::Ok => Ok(()),
            Response::Error { error } => Err(error.message),
            other => Err(format!("unexpected response: {other:?}")),
        }
    }
}

fn build_filter(config: &ClientConfig) -> MeetingsFilter {
    let filters = &config.filters;
    let mut filter = MeetingsFilter::new();

    if filters.today_only {
        filter = filter.today_only(true);
    }

    if let Some(limit) = filters.limit {
        filter = filter.limit(limit);
    }

    if filters.skip_all_day {
        filter = filter.skip_all_day(true);
    }

    if !filters.include_titles.is_empty() {
        filter = filter.include_titles(filters.include_titles.clone());
    }

    if !filters.exclude_titles.is_empty() {
        filter = filter.exclude_titles(filters.exclude_titles.clone());
    }

    if !filters.include_calendars.is_empty() {
        filter = filter.include_calendars(filters.include_calendars.clone());
    }

    if !filters.exclude_calendars.is_empty() {
        filter = filter.exclude_calendars(filters.exclude_calendars.clone());
    }

    if let Some(mins) = filters.within_minutes {
        filter = filter.within_minutes(mins);
    }

    if let Some(ref spec) = filters.work_hours {
        filter = filter.work_hours(spec.clone());
    }

    if filters.only_with_link {
        filter = filter.only_with_link(true);
    }

    if filters.privacy {
        filter = filter.privacy(true);
        if let Some(ref t) = filters.privacy_title {
            filter = filter.privacy_title(t.clone());
        }
    }

    if filters.skip_declined {
        filter = filter.skip_declined(true);
    }

    if filters.skip_tentative {
        filter = filter.skip_tentative(true);
    }

    if filters.skip_pending {
        filter = filter.skip_pending(true);
    }

    if filters.skip_without_guests {
        filter = filter.skip_without_guests(true);
    }

    filter
}

fn filter_is_empty(filter: &MeetingsFilter) -> bool {
    !filter.today_only
        && filter.limit.is_none()
        && !filter.skip_all_day
        && filter.include_titles.is_empty()
        && filter.exclude_titles.is_empty()
        && filter.include_calendars.is_empty()
        && filter.exclude_calendars.is_empty()
        && filter.within_minutes.is_none()
        && !filter.only_with_link
        && filter.work_hours.is_none()
        && !filter.privacy
        && !filter.skip_declined
        && !filter.skip_tentative
        && !filter.skip_pending
        && !filter.skip_without_guests
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_filter_maps_skip_all_day_and_exclusions() {
        let mut config = ClientConfig::default();
        config.filters.skip_all_day = true;
        config.filters.exclude_titles = vec!["home".to_string()];

        let filter = build_filter(&config);

        assert!(filter.skip_all_day);
        assert_eq!(filter.exclude_titles, vec!["home".to_string()]);
        assert!(!filter_is_empty(&filter));
    }
}
