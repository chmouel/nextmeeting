mod dismissals;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local, TimeDelta};
use serde::Serialize;
use tauri::Manager;

use nextmeeting_client::cli::{Cli, Command};
use nextmeeting_client::config::{ClientConfig, MenuBarSettings, MenuBarTitleFormat};
use nextmeeting_client::error::ClientError;
use nextmeeting_client::socket::SocketClient;
use nextmeeting_core::{MeetingView, ResponseStatus};
use nextmeeting_protocol::{Request, Response};

use dismissals::DismissedEvents;

const MENU_ID_REFRESH: &str = "menu-refresh";
const MENU_ID_PREFERENCES: &str = "menu-preferences";
const MENU_ID_QUIT: &str = "menu-quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum MeetingStatus {
    Ongoing,
    Soon,
    Upcoming,
}

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
    start_at: String,
    end_at: String,
    start_time: String,
    end_time: String,
    day_label: String,
    service: String,
    status: MeetingStatus,
    join_url: Option<String>,
    relative_time: String,
    location: Option<String>,
    description: Option<String>,
    calendar_id: String,
    attendee_count: usize,
    response_status: String,
    duration_minutes: i64,
}

type UiMeetingWithBounds<'a> = (&'a UiMeeting, Option<(DateTime<Local>, DateTime<Local>)>);

#[derive(Debug, Clone)]
struct DashboardConfig {
    mock: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
enum LaunchMode {
    #[default]
    Desktop,
    Menubar,
}


impl LaunchMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "desktop" => Some(Self::Desktop),
            "menubar" | "menu-bar" | "tray" => Some(Self::Menubar),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LaunchOptions {
    mock: bool,
    mode: LaunchMode,
}

#[tauri::command]
async fn get_dashboard_data(
    dashboard_config: tauri::State<'_, DashboardConfig>,
    dismissed: tauri::State<'_, Arc<Mutex<DismissedEvents>>>,
) -> Result<DashboardData, String> {
    let meetings = if dashboard_config.mock {
        let mut meetings = mock_meetings();
        if let Ok(d) = dismissed.lock() {
            meetings.retain(|meeting| !d.is_dismissed(&meeting.id));
        }
        meetings
    } else {
        let config = ClientConfig::load().unwrap_or_default();
        let now = Local::now();
        let mut meetings = fetch_live_meeting_views(&config).await?;
        if let Ok(d) = dismissed.lock() {
            meetings.retain(|meeting| !d.is_dismissed(&meeting.id));
        }
        meetings
            .into_iter()
            .take(8)
            .map(|meeting| map_meeting(meeting, now))
            .collect()
    };

    let source = if dashboard_config.mock { "mock" } else { "live" };
    Ok(build_dashboard(source, meetings))
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
async fn join_next_meeting(
    config: tauri::State<'_, ClientConfig>,
) -> Result<(), String> {
    let meetings = fetch_live_meeting_views(&config).await?;
    nextmeeting_client::actions::open_meeting_url(&meetings).map_err(|err| err.to_string())
}

#[tauri::command]
async fn join_meeting_by_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|err| err.to_string())
}

#[tauri::command]
async fn create_meeting(
    config: tauri::State<'_, ClientConfig>,
) -> Result<(), String> {
    let google_domain = config.google_domain.as_deref();
    nextmeeting_client::actions::create_meeting("meet", None, google_domain)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn open_calendar_day(
    config: tauri::State<'_, ClientConfig>,
) -> Result<(), String> {
    let meetings = fetch_live_meeting_views(&config).await?;
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

#[tauri::command]
async fn refresh_meetings(
    config: tauri::State<'_, ClientConfig>,
) -> Result<(), String> {
    let client = build_socket_client(&config);
    let request = Request::refresh(true);
    let response = client.send(request).await.map_err(|err| err.to_string())?;

    match response {
        Response::Ok => Ok(()),
        Response::Error { error } => Err(error.message),
        other => Err(format!("unexpected server response: {other:?}")),
    }
}

#[tauri::command]
async fn snooze_notifications(
    minutes: u32,
    config: tauri::State<'_, ClientConfig>,
) -> Result<(), String> {
    let client = build_socket_client(&config);
    let request = Request::snooze(minutes);
    let response = client.send(request).await.map_err(|err| err.to_string())?;

    match response {
        Response::Ok => Ok(()),
        Response::Error { error } => Err(error.message),
        other => Err(format!("unexpected server response: {other:?}")),
    }
}

fn build_socket_client(config: &ClientConfig) -> SocketClient {
    let socket_path = config
        .server
        .socket_path
        .clone()
        .unwrap_or_else(nextmeeting_server::default_socket_path);
    let timeout = Duration::from_secs(config.server.timeout.max(1));
    SocketClient::new(socket_path, timeout)
}

async fn fetch_live_meeting_views(config: &ClientConfig) -> Result<Vec<MeetingView>, String> {
    let client = build_socket_client(config);
    let request = Request::get_meetings();
    let response = match client.send(request.clone()).await {
        Ok(response) => response,
        Err(ClientError::Connection(_)) => {
            auto_spawn_server(&client, config).await?;
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

async fn auto_spawn_server(client: &SocketClient, config: &ClientConfig) -> Result<(), String> {
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
        let config = config.clone();
        let server_cli = Cli {
            config: None,
            debug: config.debug,
            waybar: false,
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

fn response_status_key(status: ResponseStatus) -> &'static str {
    match status {
        ResponseStatus::Accepted => "accepted",
        ResponseStatus::Declined => "declined",
        ResponseStatus::Tentative => "tentative",
        ResponseStatus::NeedsAction => "needs_action",
        ResponseStatus::Unknown => "unknown",
    }
}

fn format_relative_time(
    start: chrono::DateTime<Local>,
    end: chrono::DateTime<Local>,
    now: chrono::DateTime<Local>,
    is_ongoing: bool,
) -> String {
    if is_ongoing || (start <= now && end >= now) {
        let remaining = (end - now).num_minutes();
        if remaining <= 0 {
            "ending now".to_string()
        } else if remaining == 1 {
            "ends in 1 min".to_string()
        } else {
            format!("ends in {remaining} min")
        }
    } else if start > now {
        let until = (start - now).num_minutes();
        if until <= 0 {
            "starting now".to_string()
        } else if until == 1 {
            "starts in 1 min".to_string()
        } else if until < 60 {
            format!("starts in {until} min")
        } else {
            let hours = until / 60;
            if hours == 1 {
                "starts in 1 hr".to_string()
            } else {
                format!("starts in {hours} hrs")
            }
        }
    } else {
        String::new()
    }
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

    let relative_time = format_relative_time(
        meeting.start_local,
        meeting.end_local,
        now,
        meeting.is_ongoing,
    );

    let duration_minutes = meeting.duration_minutes();
    let response_status = response_status_key(meeting.user_response_status).to_string();
    let description = meeting.description.map(|d| {
        if d.chars().count() > 500 {
            let truncated: String = d.chars().take(497).collect();
            format!("{truncated}...")
        } else {
            d
        }
    });

    UiMeeting {
        id: meeting.id,
        title: meeting.title,
        start_at: meeting.start_local.to_rfc3339(),
        end_at: meeting.end_local.to_rfc3339(),
        start_time: meeting.start_local.format("%H:%M").to_string(),
        end_time: meeting.end_local.format("%H:%M").to_string(),
        day_label: meeting.start_local.format("%a, %-d %b").to_string(),
        service,
        status,
        join_url,
        relative_time,
        location: meeting.location,
        description,
        calendar_id: meeting.calendar_id,
        attendee_count: meeting.other_attendee_count,
        response_status,
        duration_minutes,
    }
}

fn classify_status(
    start: chrono::DateTime<Local>,
    end: chrono::DateTime<Local>,
    now: chrono::DateTime<Local>,
    is_ongoing: bool,
) -> MeetingStatus {
    if is_ongoing || (start <= now && end >= now) {
        return MeetingStatus::Ongoing;
    }

    let minutes_until_start = (start - now).num_minutes();
    if minutes_until_start <= 15 {
        MeetingStatus::Soon
    } else {
        MeetingStatus::Upcoming
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
            start_at: first_start.to_rfc3339(),
            end_at: first_end.to_rfc3339(),
            start_time: first_start.format("%H:%M").to_string(),
            end_time: first_end.format("%H:%M").to_string(),
            day_label: first_start.format("%a, %-d %b").to_string(),
            service: "Google Meet".to_string(),
            status: MeetingStatus::Soon,
            join_url: Some("https://meet.google.com/aaa-bbbb-ccc".to_string()),
            relative_time: "starts in 30 min".to_string(),
            location: Some("Conference Room A".to_string()),
            description: Some("Daily sync to discuss progress and blockers.".to_string()),
            calendar_id: "work@example.com".to_string(),
            attendee_count: 5,
            response_status: "accepted".to_string(),
            duration_minutes: 25,
        },
        UiMeeting {
            id: "mock-design".to_string(),
            title: "Product design sync".to_string(),
            start_at: second_start.to_rfc3339(),
            end_at: second_end.to_rfc3339(),
            start_time: second_start.format("%H:%M").to_string(),
            end_time: second_end.format("%H:%M").to_string(),
            day_label: second_start.format("%a, %-d %b").to_string(),
            service: "Zoom".to_string(),
            status: MeetingStatus::Upcoming,
            join_url: Some("https://zoom.us/j/123456789".to_string()),
            relative_time: "starts in 2 hrs".to_string(),
            location: None,
            description: Some("Review new mockups for the dashboard redesign.".to_string()),
            calendar_id: "design@example.com".to_string(),
            attendee_count: 3,
            response_status: "tentative".to_string(),
            duration_minutes: 45,
        },
    ]
}

fn parse_launch_options<I, S>(args: I, mode_from_env: Option<&str>) -> LaunchOptions
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut mock = false;
    let mut mode_from_args = None;

    for arg in args {
        match arg.as_ref() {
            "--mock" => mock = true,
            "--menubar" => mode_from_args = Some(LaunchMode::Menubar),
            "--desktop" => mode_from_args = Some(LaunchMode::Desktop),
            _ => {}
        }
    }

    let env_mode = mode_from_env.and_then(LaunchMode::parse);
    let mode = mode_from_args.or(env_mode).unwrap_or_default();

    LaunchOptions { mock, mode }
}

#[cfg(target_os = "linux")]
fn effective_launch_mode(mode: LaunchMode) -> LaunchMode {
    match mode {
        LaunchMode::Menubar => LaunchMode::Desktop,
        other => other,
    }
}

#[cfg(not(target_os = "linux"))]
fn effective_launch_mode(mode: LaunchMode) -> LaunchMode {
    mode
}

fn truncate_title(title: &str, max_len: Option<usize>) -> String {
    let max = max_len.unwrap_or(40);
    if max == 0 {
        return title.to_string();
    }
    let chars: Vec<char> = title.chars().collect();
    if chars.len() <= max {
        title.to_string()
    } else {
        let truncated: String = chars[..max.saturating_sub(1)].iter().collect();
        format!("{truncated}\u{2026}")
    }
}

fn menubar_title_from_meetings(
    meetings: &[MeetingView],
    settings: &MenuBarSettings,
) -> Option<String> {
    let now = Local::now();

    let candidates: Vec<&MeetingView> = meetings
        .iter()
        .filter(|meeting| match settings.event_threshold_minutes {
            Some(threshold) => meeting.is_ongoing || meeting.minutes_until_start(now) <= threshold as i64,
            None => true,
        })
        .collect();
    let meeting = candidates
        .iter()
        .copied()
        .find(|meeting| meeting.is_ongoing)
        .or_else(|| candidates.iter().copied().min_by_key(|meeting| meeting.start_local));
    let Some(meeting) = meeting else {
        return if settings.event_threshold_minutes.is_some() {
            Some("\u{25CF}".to_string())
        } else {
            None
        };
    };
    let title = meeting.title.trim();
    if title.is_empty() {
        return None;
    }

    let result = match settings.title_format {
        MenuBarTitleFormat::Full => truncate_title(title, settings.title_max_length),
        MenuBarTitleFormat::Dot => "\u{25CF}".to_string(),
        MenuBarTitleFormat::Hidden => return None,
    };

    if !settings.show_time || !matches!(settings.title_format, MenuBarTitleFormat::Full) {
        return Some(result);
    }

    let suffix = if meeting.is_ongoing {
        let remaining = meeting.minutes_until_end(now).max(0);
        format!(" ({remaining}m left)")
    } else {
        let until = meeting.minutes_until_start(now).max(0);
        format!(" (in {until}m)")
    };
    Some(format!("{result}{suffix}"))
}

fn parse_ui_meeting_bounds(meeting: &UiMeeting) -> Option<(DateTime<Local>, DateTime<Local>)> {
    let start = DateTime::parse_from_rfc3339(&meeting.start_at)
        .ok()?
        .with_timezone(&Local);
    let mut end = DateTime::parse_from_rfc3339(&meeting.end_at)
        .ok()?
        .with_timezone(&Local);
    if end <= start {
        end += TimeDelta::days(1);
    }
    Some((start, end))
}

fn menubar_title_from_ui_meetings(
    meetings: &[UiMeeting],
    settings: &MenuBarSettings,
) -> Option<String> {
    let now = Local::now();
    let candidates: Vec<UiMeetingWithBounds<'_>> = meetings
        .iter()
        .filter_map(|meeting| {
            let bounds = parse_ui_meeting_bounds(meeting);
            let is_ongoing = meeting.status == MeetingStatus::Ongoing;
            let within_threshold = match settings.event_threshold_minutes {
                Some(threshold) => bounds
                    .as_ref()
                    .map(|(start, _)| (*start - now).num_minutes() <= threshold as i64)
                    .unwrap_or(false),
                None => true,
            };

            if settings.event_threshold_minutes.is_some() && !is_ongoing && !within_threshold {
                return None;
            }

            Some((meeting, bounds))
        })
        .collect();
    let meeting = candidates
        .iter()
        .find(|(meeting, _)| meeting.status == MeetingStatus::Ongoing)
        .or_else(|| {
            candidates
                .iter()
                .filter(|(_, bounds)| bounds.is_some())
                .min_by_key(|(_, bounds)| bounds.map(|(start, _)| start))
        })
        .or_else(|| candidates.first())
        .copied();
    let Some((meeting, bounds)) = meeting else {
        return if settings.event_threshold_minutes.is_some() {
            Some("\u{25CF}".to_string())
        } else {
            None
        };
    };
    let title = meeting.title.trim();
    if title.is_empty() {
        return None;
    }

    let result = match settings.title_format {
        MenuBarTitleFormat::Full => Some(truncate_title(title, settings.title_max_length)),
        MenuBarTitleFormat::Dot => Some("\u{25CF}".to_string()),
        MenuBarTitleFormat::Hidden => None,
    };
    let result = result?;

    if !settings.show_time || !matches!(settings.title_format, MenuBarTitleFormat::Full) {
        return Some(result);
    }

    let suffix = if meeting.status == MeetingStatus::Ongoing {
        let remaining = bounds
            .map(|(_, end)| (end - now).num_minutes().max(0))
            .unwrap_or(0);
        format!(" ({remaining}m left)")
    } else {
        let until = bounds
            .map(|(start, _)| (start - now).num_minutes().max(0))
            .unwrap_or(0);
        format!(" (in {until}m)")
    };
    Some(format!("{result}{suffix}"))
}

#[cfg(target_os = "macos")]
fn menubar_template_icon() -> tauri::image::Image<'static> {
    tauri::image::Image::new(
        include_bytes!("../icons/icon-menubar-tight-18.rgba"),
        18,
        18,
    )
}

#[cfg(target_os = "macos")]
async fn refresh_menubar_title(app: &tauri::AppHandle) {
    let config = app
        .try_state::<ClientConfig>()
        .map(|c| c.inner().clone())
        .unwrap_or_default();
    let menubar_settings = &config.menubar;

    let title = match app.try_state::<DashboardConfig>() {
        Some(dc) if dc.mock => {
            let mut meetings = mock_meetings();
            if let Some(dismissed) = app.try_state::<Arc<Mutex<DismissedEvents>>>()
                && let Ok(d) = dismissed.lock()
            {
                meetings.retain(|meeting| !d.is_dismissed(&meeting.id));
            }
            menubar_title_from_ui_meetings(&meetings, menubar_settings)
        }
        _ => match fetch_live_meeting_views(&config).await {
            Ok(mut meetings) => {
                if let Some(dismissed) = app.try_state::<Arc<Mutex<DismissedEvents>>>()
                    && let Ok(d) = dismissed.lock()
                {
                    meetings.retain(|meeting| !d.is_dismissed(&meeting.id));
                }
                menubar_title_from_meetings(&meetings, menubar_settings)
            }
            Err(_) => None,
        },
    };

    if let Some(tray) = app.tray_by_id("nextmeeting-tray") {
        let _ = tray.set_title(title.as_deref());
    }
}

#[cfg(target_os = "macos")]
fn spawn_menubar_title_updater(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_menubar_title(&app).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn spawn_menubar_title_updater(_app: tauri::AppHandle) {}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    window.show()?;
    window.unminimize()?;
    window.set_focus()?;
    Ok(())
}

fn hide_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    window.hide()?;
    Ok(())
}

fn toggle_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    if window.is_visible()? {
        window.hide()?;
    } else {
        show_main_window(app)?;
    }
    Ok(())
}

fn setup_menubar_mode(app: &mut tauri::App<tauri::Wry>) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        app.set_dock_visibility(false);
    }

    hide_main_window(app.handle())?;
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "linux")]
        let _ = window.set_always_on_top(false);
        #[cfg(not(target_os = "linux"))]
        let _ = window.set_always_on_top(true);
    }

    let show_item = tauri::menu::MenuItem::with_id(app, "toggle-window", "Toggle window", true, None::<&str>)?;
    let quit_item = tauri::menu::MenuItem::with_id(app, "quit-app", "Quit", true, None::<&str>)?;
    let tray_menu = tauri::menu::Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut tray_builder = tauri::tray::TrayIconBuilder::with_id("nextmeeting-tray")
        .menu(&tray_menu)
        .tooltip("nextmeeting")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let _ = toggle_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            if event.id() == "toggle-window" {
                let _ = toggle_main_window(app);
            } else if event.id() == "quit-app" {
                app.exit(0);
            }
        });

    #[cfg(target_os = "macos")]
    {
        tray_builder = tray_builder
            .icon(menubar_template_icon())
            .icon_as_template(true);
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(icon) = app.default_window_icon().cloned() {
            tray_builder = tray_builder.icon(icon);
        }
    }

    let _tray = tray_builder.build(app)?;
    spawn_menubar_title_updater(app.handle().clone());
    Ok(())
}

fn setup_app_menu(app: &mut tauri::App<tauri::Wry>) -> tauri::Result<()> {
    let preferences_item = tauri::menu::MenuItem::with_id(
        app,
        MENU_ID_PREFERENCES,
        "Preferences",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let refresh_item = tauri::menu::MenuItem::with_id(
        app,
        MENU_ID_REFRESH,
        "Refresh",
        true,
        Some("CmdOrCtrl+R"),
    )?;
    let quit_item = tauri::menu::MenuItem::with_id(
        app,
        MENU_ID_QUIT,
        "Quit nextmeeting",
        true,
        Some("CmdOrCtrl+Q"),
    )?;

    #[cfg(target_os = "macos")]
    let menu = {
        let app_submenu = tauri::menu::Submenu::with_items(
            app,
            "nextmeeting",
            true,
            &[&preferences_item, &refresh_item, &quit_item],
        )?;
        tauri::menu::Menu::with_items(app, &[&app_submenu])?
    };

    #[cfg(not(target_os = "macos"))]
    let menu = {
        let file_submenu = tauri::menu::Submenu::with_items(
            app,
            "File",
            true,
            &[&preferences_item, &refresh_item, &quit_item],
        )?;
        tauri::menu::Menu::with_items(app, &[&file_submenu])?
    };

    app.set_menu(menu)?;
    Ok(())
}

fn handle_menu_event(app: &tauri::AppHandle, menu_id: &str) {
    match menu_id {
        MENU_ID_PREFERENCES => {
            tauri::async_runtime::spawn(async {
                let _ = open_preferences().await;
            });
        }
        MENU_ID_REFRESH => {
            if let Some(config) = app.try_state::<ClientConfig>().map(|state| state.inner().clone())
            {
                tauri::async_runtime::spawn(async move {
                    let client = build_socket_client(&config);
                    let request = Request::refresh(true);
                    let _ = client.send(request).await;
                });
            }
        }
        MENU_ID_QUIT => app.exit(0),
        _ => {}
    }
}

fn configure_builder(
    builder: tauri::Builder<tauri::Wry>,
    launch_mode: LaunchMode,
) -> tauri::Builder<tauri::Wry> {
    match launch_mode {
        LaunchMode::Desktop => builder
            .setup(|app| {
                setup_app_menu(app)?;
                Ok(())
            })
            .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref())),
        LaunchMode::Menubar => builder
            .setup(|app| {
                setup_app_menu(app)?;
                setup_menubar_mode(app)?;
                Ok(())
            })
            .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
            .on_window_event(|window, event| {
                if window.label() != "main" {
                    return;
                }
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = hide_main_window(window.app_handle());
                    }
                    tauri::WindowEvent::Focused(false) => {
                        let _ = hide_main_window(window.app_handle());
                    }
                    _ => {}
                }
            }),
    }
}

#[tauri::command]
fn dismiss_event(
    event_id: String,
    dismissed: tauri::State<'_, Arc<Mutex<DismissedEvents>>>,
) -> Result<(), String> {
    dismissed
        .lock()
        .map_err(|e| e.to_string())?
        .dismiss(event_id);
    Ok(())
}

#[tauri::command]
fn undismiss_event(
    event_id: String,
    dismissed: tauri::State<'_, Arc<Mutex<DismissedEvents>>>,
) -> Result<(), String> {
    dismissed
        .lock()
        .map_err(|e| e.to_string())?
        .undismiss(&event_id);
    Ok(())
}

#[tauri::command]
fn clear_dismissals(
    dismissed: tauri::State<'_, Arc<Mutex<DismissedEvents>>>,
) -> Result<(), String> {
    dismissed
        .lock()
        .map_err(|e| e.to_string())?
        .clear();
    Ok(())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

fn main() {
    let env_mode = std::env::var("NEXTMEETING_GUI_MODE").ok();
    let launch = parse_launch_options(std::env::args(), env_mode.as_deref());
    let launch_mode = effective_launch_mode(launch.mode);
    let config = ClientConfig::load().unwrap_or_default();
    let dismissed = Arc::new(Mutex::new(DismissedEvents::load()));

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(DashboardConfig { mock: launch.mock })
        .manage(config)
        .manage(dismissed)
        .invoke_handler(tauri::generate_handler![
            get_dashboard_data,
            join_next_meeting,
            join_meeting_by_url,
            create_meeting,
            open_calendar_day,
            open_preferences,
            refresh_meetings,
            snooze_notifications,
            dismiss_event,
            undismiss_event,
            clear_dismissals,
            quit_app
        ]);

    configure_builder(builder, launch_mode)
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

        assert_eq!(status, MeetingStatus::Ongoing);
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

        assert_eq!(status, MeetingStatus::Soon);
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

        assert_eq!(status, MeetingStatus::Upcoming);
    }

    #[test]
    fn parse_launch_options_default_to_desktop() {
        let args = vec!["nextmeeting-gui"];
        let options = parse_launch_options(args, None);

        assert_eq!(options.mode, LaunchMode::Desktop);
        assert!(!options.mock);
    }

    #[test]
    fn parse_launch_options_accepts_menubar_flag() {
        let args = vec!["nextmeeting-gui", "--menubar"];
        let options = parse_launch_options(args, None);

        assert_eq!(options.mode, LaunchMode::Menubar);
    }

    #[test]
    fn parse_launch_options_reads_environment_mode() {
        let args = vec!["nextmeeting-gui"];
        let options = parse_launch_options(args, Some("menubar"));

        assert_eq!(options.mode, LaunchMode::Menubar);
    }

    #[test]
    fn parse_launch_options_prefers_cli_over_environment() {
        let args = vec!["nextmeeting-gui", "--desktop"];
        let options = parse_launch_options(args, Some("menubar"));

        assert_eq!(options.mode, LaunchMode::Desktop);
    }

    #[test]
    fn effective_launch_mode_keeps_desktop() {
        assert_eq!(effective_launch_mode(LaunchMode::Desktop), LaunchMode::Desktop);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn effective_launch_mode_disables_menubar_on_linux() {
        assert_eq!(effective_launch_mode(LaunchMode::Menubar), LaunchMode::Desktop);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn effective_launch_mode_keeps_menubar_off_linux() {
        assert_eq!(effective_launch_mode(LaunchMode::Menubar), LaunchMode::Menubar);
    }

    #[test]
    fn menubar_title_prefers_ongoing_meeting() {
        let now = Local::now();
        let settings = MenuBarSettings { show_time: false, ..Default::default() };
        let meetings = vec![
            MeetingView {
                id: "upcoming".to_string(),
                title: "Upcoming".to_string(),
                start_local: now + TimeDelta::minutes(30),
                end_local: now + TimeDelta::minutes(60),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: nextmeeting_core::ResponseStatus::Accepted,
                other_attendee_count: 1,
                location: None,
                description: None,
            },
            MeetingView {
                id: "ongoing".to_string(),
                title: "Live now".to_string(),
                start_local: now - TimeDelta::minutes(10),
                end_local: now + TimeDelta::minutes(20),
                is_all_day: false,
                is_ongoing: true,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: nextmeeting_core::ResponseStatus::Accepted,
                other_attendee_count: 1,
                location: None,
                description: None,
            },
        ];

        assert_eq!(
            menubar_title_from_meetings(&meetings, &settings),
            Some("Live now".to_string())
        );
    }

    #[test]
    fn menubar_title_is_none_without_meetings() {
        let settings = MenuBarSettings::default();
        assert_eq!(menubar_title_from_meetings(&[], &settings), None);
    }

    #[test]
    fn menubar_title_from_ui_meetings_uses_first_title() {
        let settings = MenuBarSettings {
            show_time: false,
            ..Default::default()
        };
        let meetings = vec![UiMeeting {
            id: "mock-1".to_string(),
            title: "Engineering stand-up".to_string(),
            start_at: Local::now().to_rfc3339(),
            end_at: (Local::now() + TimeDelta::minutes(25)).to_rfc3339(),
            start_time: "22:30".to_string(),
            end_time: "22:55".to_string(),
            day_label: "Sat, 7 Feb".to_string(),
            service: "Google Meet".to_string(),
            status: MeetingStatus::Soon,
            join_url: None,
            relative_time: "starts in 30 min".to_string(),
            location: None,
            description: None,
            calendar_id: "primary".to_string(),
            attendee_count: 0,
            response_status: "unknown".to_string(),
            duration_minutes: 25,
        }];

        assert_eq!(
            menubar_title_from_ui_meetings(&meetings, &settings),
            Some("Engineering stand-up".to_string())
        );
    }

    #[test]
    fn menubar_title_dot_format() {
        let now = Local::now();
        let settings = MenuBarSettings {
            title_format: MenuBarTitleFormat::Dot,
            show_time: false,
            ..Default::default()
        };
        let meetings = vec![MeetingView {
            id: "1".to_string(),
            title: "Standup".to_string(),
            start_local: now + TimeDelta::minutes(10),
            end_local: now + TimeDelta::minutes(40),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            calendar_id: "primary".to_string(),
            user_response_status: nextmeeting_core::ResponseStatus::Accepted,
            other_attendee_count: 1,
            location: None,
            description: None,
        }];

        assert_eq!(
            menubar_title_from_meetings(&meetings, &settings),
            Some("\u{25CF}".to_string())
        );
    }

    #[test]
    fn menubar_title_hidden_format() {
        let now = Local::now();
        let settings = MenuBarSettings {
            title_format: MenuBarTitleFormat::Hidden,
            ..Default::default()
        };
        let meetings = vec![MeetingView {
            id: "1".to_string(),
            title: "Standup".to_string(),
            start_local: now + TimeDelta::minutes(10),
            end_local: now + TimeDelta::minutes(40),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            calendar_id: "primary".to_string(),
            user_response_status: nextmeeting_core::ResponseStatus::Accepted,
            other_attendee_count: 1,
            location: None,
            description: None,
        }];

        assert_eq!(menubar_title_from_meetings(&meetings, &settings), None);
    }

    #[test]
    fn menubar_title_truncation() {
        let now = Local::now();
        let settings = MenuBarSettings {
            title_max_length: Some(10),
            show_time: false,
            ..Default::default()
        };
        let meetings = vec![MeetingView {
            id: "1".to_string(),
            title: "A very long meeting title here".to_string(),
            start_local: now + TimeDelta::minutes(10),
            end_local: now + TimeDelta::minutes(40),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            calendar_id: "primary".to_string(),
            user_response_status: nextmeeting_core::ResponseStatus::Accepted,
            other_attendee_count: 1,
            location: None,
            description: None,
        }];

        let title = menubar_title_from_meetings(&meetings, &settings).unwrap();
        assert!(title.chars().count() <= 10);
        assert!(title.ends_with('\u{2026}'));
    }

    #[test]
    fn menubar_title_threshold_filters() {
        let now = Local::now();
        let settings = MenuBarSettings {
            event_threshold_minutes: Some(15),
            show_time: false,
            ..Default::default()
        };
        let meetings = vec![MeetingView {
            id: "1".to_string(),
            title: "Far away".to_string(),
            start_local: now + TimeDelta::minutes(60),
            end_local: now + TimeDelta::minutes(90),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            calendar_id: "primary".to_string(),
            user_response_status: nextmeeting_core::ResponseStatus::Accepted,
            other_attendee_count: 1,
            location: None,
            description: None,
        }];

        assert_eq!(
            menubar_title_from_meetings(&meetings, &settings),
            Some("\u{25CF}".to_string())
        );
    }

    #[test]
    fn format_relative_time_ongoing() {
        let now = Local::now();
        let result = format_relative_time(
            now - TimeDelta::minutes(10),
            now + TimeDelta::minutes(20),
            now,
            true,
        );
        assert_eq!(result, "ends in 20 min");
    }

    #[test]
    fn format_relative_time_soon() {
        let now = Local::now();
        let result = format_relative_time(
            now + TimeDelta::minutes(12),
            now + TimeDelta::minutes(42),
            now,
            false,
        );
        assert_eq!(result, "starts in 12 min");
    }

    #[test]
    fn format_relative_time_hours() {
        let now = Local::now();
        let result = format_relative_time(
            now + TimeDelta::hours(3),
            now + TimeDelta::hours(4),
            now,
            false,
        );
        assert_eq!(result, "starts in 3 hrs");
    }

    #[test]
    fn response_status_key_uses_snake_case_values() {
        assert_eq!(response_status_key(ResponseStatus::Accepted), "accepted");
        assert_eq!(response_status_key(ResponseStatus::Declined), "declined");
        assert_eq!(response_status_key(ResponseStatus::Tentative), "tentative");
        assert_eq!(response_status_key(ResponseStatus::NeedsAction), "needs_action");
        assert_eq!(response_status_key(ResponseStatus::Unknown), "unknown");
    }
}
