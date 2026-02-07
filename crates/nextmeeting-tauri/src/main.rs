use std::path::Path;
use std::time::Duration;

use chrono::{Local, TimeDelta};
use serde::Serialize;

use nextmeeting_client::cli::{Cli, Command};
use nextmeeting_client::config::ClientConfig;
use nextmeeting_client::error::ClientError;
use nextmeeting_client::socket::SocketClient;
use nextmeeting_core::MeetingView;
use nextmeeting_protocol::{Request, Response};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardData {
    source: String,
    generated_at: String,
    meetings: Vec<UiMeeting>,
    actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiMeeting {
    id: String,
    title: String,
    start_time: String,
    end_time: String,
    day_label: String,
    service: String,
    status: String,
    join_url: Option<String>,
}

#[derive(Debug, Clone)]
struct DashboardConfig {
    mock: bool,
}

#[tauri::command]
async fn get_dashboard_data(
    dashboard_config: tauri::State<'_, DashboardConfig>,
) -> Result<DashboardData, String> {
    if dashboard_config.mock {
        return Ok(build_dashboard("mock", mock_meetings()));
    }

    let meetings = fetch_live_meetings_ui().await?;
    Ok(build_dashboard("live", meetings))
}

fn build_dashboard(source: &str, meetings: Vec<UiMeeting>) -> DashboardData {
    DashboardData {
        source: source.to_string(),
        generated_at: Local::now().to_rfc3339(),
        meetings,
        actions: vec![
            "Join next meeting".to_string(),
            "Create meeting".to_string(),
            "Quick Actions".to_string(),
            "Preferences".to_string(),
            "Quit".to_string(),
        ],
    }
}

#[tauri::command]
async fn join_next_meeting() -> Result<(), String> {
    let meetings = fetch_live_meeting_views().await?;
    nextmeeting_client::actions::open_meeting_url(&meetings).map_err(|err| err.to_string())
}

#[tauri::command]
async fn create_meeting() -> Result<(), String> {
    let config = ClientConfig::load().unwrap_or_default();
    let google_domain = config.google_domain.as_deref();
    nextmeeting_client::actions::create_meeting("meet", None, google_domain)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn open_calendar_day() -> Result<(), String> {
    let meetings = fetch_live_meeting_views().await?;
    let config = ClientConfig::load().unwrap_or_default();
    let google_domain = config.google_domain.as_deref();
    nextmeeting_client::actions::open_calendar_day(&meetings, google_domain)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn open_preferences() -> Result<(), String> {
    let config_path = ClientConfig::default_path();
    ensure_parent_dir(&config_path)?;
    if !config_path.exists() {
        std::fs::write(&config_path, b"").map_err(|err| err.to_string())?;
    }

    open::that(&config_path).map_err(|err| err.to_string())?;
    Ok(())
}

async fn fetch_live_meetings_ui() -> Result<Vec<UiMeeting>, String> {
    let meetings = fetch_live_meeting_views().await?;
    let now = Local::now();

    Ok(meetings
        .into_iter()
        .take(8)
        .map(|meeting| map_meeting(meeting, now))
        .collect())
}

async fn fetch_live_meeting_views() -> Result<Vec<MeetingView>, String> {
    let config = ClientConfig::load().unwrap_or_default();
    let socket_path = config
        .server
        .socket_path
        .unwrap_or_else(nextmeeting_server::default_socket_path);
    let timeout = Duration::from_secs(config.server.timeout.max(1));

    let client = SocketClient::new(socket_path, timeout);
    let request = Request::get_meetings();
    let response = match client.send(request.clone()).await {
        Ok(response) => response,
        Err(ClientError::Connection(_)) => {
            auto_spawn_server(&client).await?;
            client.send(request).await.map_err(|err| err.to_string())?
        }
        Err(err) => return Err(err.to_string()),
    };

    match response {
        Response::Meetings { meetings } => Ok(meetings),
        Response::Error { error } => Err(error.message),
        other => Err(format!("unexpected server response: {other:?}")),
    }
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|err| err.to_string())
}

async fn auto_spawn_server(client: &SocketClient) -> Result<(), String> {
    use tokio::process::Command as TokioCommand;

    let mut cmd = TokioCommand::new("nextmeeting");
    cmd.arg("server");
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        // SAFETY: setsid is used in pre_exec to detach the server process.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let spawn_result = cmd.spawn();
    if spawn_result.is_err() {
        let config = ClientConfig::load().unwrap_or_default();
        let server_cli = Cli {
            config: None,
            debug: config.debug,
            waybar: false,
            polybar: false,
            json: false,
            privacy: false,
            snooze: None,
            open_meet_url: false,
            copy_meeting_url: false,
            copy_meeting_id: false,
            copy_meeting_passcode: false,
            open_calendar_day: false,
            open_link_from_clipboard: false,
            create: None,
            create_url: None,
            refresh: false,
            socket_path: config.server.socket_path.clone(),
            command: Some(Command::Server),
        };
        tokio::spawn(async move {
            let _ = nextmeeting_client::commands::server::run(&server_cli, &config).await;
        });
    }

    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if client.socket_exists()
            && let Ok(true) = client.ping().await
        {
            return Ok(());
        }
    }

    Err("server failed to start within timeout".to_string())
}

fn map_meeting(meeting: MeetingView, now: chrono::DateTime<Local>) -> UiMeeting {
    let status = classify_status(
        meeting.start_local,
        meeting.end_local,
        now,
        meeting.is_ongoing,
    );
    let service = meeting
        .primary_link
        .as_ref()
        .map(|link| link.kind.display_name().to_string())
        .unwrap_or_else(|| "Calendar".to_string());
    let join_url = meeting
        .primary_link
        .as_ref()
        .map(|link| link.url.clone())
        .or(meeting.calendar_url.clone());

    UiMeeting {
        id: meeting.id,
        title: meeting.title,
        start_time: meeting.start_local.format("%H:%M").to_string(),
        end_time: meeting.end_local.format("%H:%M").to_string(),
        day_label: meeting.start_local.format("%a, %-d %b").to_string(),
        service,
        status,
        join_url,
    }
}

fn classify_status(
    start: chrono::DateTime<Local>,
    end: chrono::DateTime<Local>,
    now: chrono::DateTime<Local>,
    is_ongoing: bool,
) -> String {
    if is_ongoing || (start <= now && end >= now) {
        return "ongoing".to_string();
    }

    let minutes_until_start = (start - now).num_minutes();
    if minutes_until_start <= 15 {
        "soon".to_string()
    } else {
        "upcoming".to_string()
    }
}

fn mock_meetings() -> Vec<UiMeeting> {
    let now = Local::now();
    let first_start = now + TimeDelta::minutes(30);
    let first_end = first_start + TimeDelta::minutes(25);
    let second_start = now + TimeDelta::hours(2);
    let second_end = second_start + TimeDelta::minutes(45);

    vec![
        UiMeeting {
            id: "mock-standup".to_string(),
            title: "Engineering stand-up".to_string(),
            start_time: first_start.format("%H:%M").to_string(),
            end_time: first_end.format("%H:%M").to_string(),
            day_label: first_start.format("%a, %-d %b").to_string(),
            service: "Google Meet".to_string(),
            status: "soon".to_string(),
            join_url: Some("https://meet.google.com/aaa-bbbb-ccc".to_string()),
        },
        UiMeeting {
            id: "mock-design".to_string(),
            title: "Product design sync".to_string(),
            start_time: second_start.format("%H:%M").to_string(),
            end_time: second_end.format("%H:%M").to_string(),
            day_label: second_start.format("%a, %-d %b").to_string(),
            service: "Zoom".to_string(),
            status: "upcoming".to_string(),
            join_url: Some("https://zoom.us/j/123456789".to_string()),
        },
    ]
}

fn use_mock_from_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--mock")
}

fn main() {
    let mock = use_mock_from_args(std::env::args());

    tauri::Builder::default()
        .manage(DashboardConfig { mock })
        .invoke_handler(tauri::generate_handler![
            get_dashboard_data,
            join_next_meeting,
            create_meeting,
            open_calendar_day,
            open_preferences
        ])
        .run(tauri::generate_context!())
        .expect("failed to run nextmeeting GUI");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_status_for_ongoing_meeting() {
        let now = Local::now();
        let status = classify_status(
            now - TimeDelta::minutes(5),
            now + TimeDelta::minutes(20),
            now,
            true,
        );

        assert_eq!(status, "ongoing");
    }

    #[test]
    fn classify_status_for_soon_meeting() {
        let now = Local::now();
        let status = classify_status(
            now + TimeDelta::minutes(10),
            now + TimeDelta::minutes(40),
            now,
            false,
        );

        assert_eq!(status, "soon");
    }

    #[test]
    fn classify_status_for_upcoming_meeting() {
        let now = Local::now();
        let status = classify_status(
            now + TimeDelta::minutes(60),
            now + TimeDelta::minutes(90),
            now,
            false,
        );

        assert_eq!(status, "upcoming");
    }

    #[test]
    fn use_mock_from_args_detects_hidden_flag() {
        let args = vec!["nextmeeting-gui", "--mock"];
        assert!(use_mock_from_args(args));
    }

    #[test]
    fn use_mock_from_args_ignores_other_flags() {
        let args = vec!["nextmeeting-gui", "--help"];
        assert!(!use_mock_from_args(args));
    }
}
