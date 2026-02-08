use std::rc::Rc;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use crate::config::GtkConfig;
use crate::tray::{manager::TrayManager, TrayCommand};
use crate::utils::{format_time_range, truncate};
use crate::widgets::window::{build as build_window, UiWidgets};

#[derive(Debug)]
pub struct AppRuntime {
    config: GtkConfig,
    daemon: crate::daemon::client::DaemonClient,
    pub state: crate::daemon::state::MeetingState,
    dismissals: crate::dismissals::DismissedEvents,
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRuntime {
    pub fn new() -> Self {
        let config = GtkConfig::load();
        let daemon = crate::daemon::client::DaemonClient::from_config(&config.client);
        Self {
            config,
            daemon,
            state: crate::daemon::state::MeetingState::default(),
            dismissals: crate::dismissals::DismissedEvents::load(),
        }
    }

    pub async fn refresh(&mut self) -> Result<usize, String> {
        let meetings = self.daemon.get_meetings().await?;
        let visible: Vec<_> = meetings
            .into_iter()
            .filter(|meeting| !self.dismissals.is_dismissed(&meeting.id))
            .collect();
        let count = visible.len();
        self.state.set_meetings(visible);
        Ok(count)
    }

    pub async fn force_refresh(&self) -> Result<(), String> {
        self.daemon.refresh().await
    }

    pub async fn snooze(&self, minutes: u32) -> Result<(), String> {
        self.daemon.snooze(minutes).await
    }

    pub fn dismiss_event(&mut self, event_id: &str) {
        self.dismissals.dismiss(event_id.to_string());
        self.state.remove_meeting(event_id);
    }

    pub fn clear_dismissals(&mut self) {
        self.dismissals.clear();
    }

    pub fn open_next_meeting(&self) -> Result<(), String> {
        nextmeeting_client::actions::open_meeting_url(self.state.meetings()).map_err(|e| e.to_string())
    }

    pub fn create_meeting(&self, service: &str, custom_url: Option<&str>) -> Result<(), String> {
        let domain = self.config.client.google_domain.as_deref();
        nextmeeting_client::actions::create_meeting(service, custom_url, domain)
            .map_err(|e| e.to_string())
    }

    pub fn open_calendar_day(&self) -> Result<(), String> {
        let domain = self.config.client.google_domain.as_deref();
        nextmeeting_client::actions::open_calendar_day(self.state.meetings(), domain)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug)]
pub struct GtkApp {
    runtime: Arc<Runtime>,
    app_runtime: Arc<Mutex<AppRuntime>>,
}

#[derive(Debug)]
enum UiEvent {
    MeetingsLoaded(Vec<nextmeeting_core::MeetingView>),
    ActionFailed(String),
    ActionSucceeded(String),
}

impl GtkApp {
    pub fn new() -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("failed to create runtime: {e}"))?;

        Ok(Self {
            runtime: Arc::new(runtime),
            app_runtime: Arc::new(Mutex::new(AppRuntime::new())),
        })
    }

    pub fn run(self) -> glib::ExitCode {
        adw::init().expect("failed to initialise libadwaita");

        let app = adw::Application::builder()
            .application_id("com.chmouel.nextmeeting")
            .build();

        let runtime = self.runtime.clone();
        let app_runtime = self.app_runtime.clone();

        app.connect_activate(move |app| {
            build_ui(app, runtime.clone(), app_runtime.clone());
        });

        app.run()
    }
}

fn build_ui(app: &adw::Application, runtime: Arc<Runtime>, app_runtime: Arc<Mutex<AppRuntime>>) {
    let widgets = Rc::new(build_window(app));

    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("../resources/style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk4::prelude::RootExt::display(&widgets.window),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>();

    {
        let widgets_for_events = widgets.clone();
        let runtime_for_events = runtime.clone();
        let app_runtime_for_events = app_runtime.clone();
        let ui_tx_for_events = ui_tx.clone();

        glib::source::timeout_add_local(Duration::from_millis(120), move || {
            while let Ok(event) = ui_rx.try_recv() {
                match event {
                    UiEvent::MeetingsLoaded(meetings) => {
                        widgets_for_events
                            .status_label
                            .set_label(&format!("Connected • {} meeting(s)", meetings.len()));
                        widgets_for_events
                            .join_button
                            .set_sensitive(meetings.iter().any(|m| m.primary_link.is_some()));
                        update_hero(&widgets_for_events, &meetings);

                        render_meetings(
                            &widgets_for_events,
                            &meetings,
                            runtime_for_events.clone(),
                            app_runtime_for_events.clone(),
                            ui_tx_for_events.clone(),
                        );
                    }
                    UiEvent::ActionFailed(err) => {
                        widgets_for_events.status_label.set_label(&format!("Error • {err}"));
                    }
                    UiEvent::ActionSucceeded(msg) => {
                        widgets_for_events.status_label.set_label(&msg);
                    }
                }
            }

            glib::ControlFlow::Continue
        });
    }

    connect_actions(&widgets, runtime.clone(), app_runtime.clone(), ui_tx.clone());

    let (tray_tx, tray_rx) = mpsc::channel::<TrayCommand>();
    let tray_manager = TrayManager::new(runtime.clone(), tray_tx);
    tray_manager.start();

    {
        let widgets_for_tray = widgets.clone();
        let runtime_for_tray = runtime.clone();
        let app_runtime_for_tray = app_runtime.clone();
        let ui_tx_for_tray = ui_tx.clone();
        let app_for_tray = app.clone();

        glib::source::timeout_add_local(Duration::from_millis(200), move || {
            while let Ok(cmd) = tray_rx.try_recv() {
                match cmd {
                    TrayCommand::ToggleWindow => {
                        if widgets_for_tray.window.is_visible() {
                            widgets_for_tray.window.set_visible(false);
                        } else {
                            widgets_for_tray.window.present();
                        }
                    }
                    TrayCommand::Refresh => {
                        trigger_refresh(
                            runtime_for_tray.clone(),
                            app_runtime_for_tray.clone(),
                            ui_tx_for_tray.clone(),
                        );
                    }
                    TrayCommand::Quit => {
                        app_for_tray.quit();
                    }
                }
            }

            glib::ControlFlow::Continue
        });
    }

    trigger_refresh(runtime, app_runtime, ui_tx);
    widgets.window.present();
}

fn connect_actions(
    widgets: &Rc<UiWidgets>,
    runtime: Arc<Runtime>,
    app_runtime: Arc<Mutex<AppRuntime>>,
    ui_tx: mpsc::Sender<UiEvent>,
) {
    {
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets.refresh_button.connect_clicked(move |_| {
            trigger_refresh(runtime.clone(), app_runtime.clone(), ui_tx.clone());
        });
    }

    {
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets.join_button.connect_clicked(move |_| {
            runtime.spawn({
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                async move {
                    let guard = app_runtime.lock().await;
                    match guard.open_next_meeting() {
                        Ok(()) => {
                            let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                "Opened next meeting URL".to_string(),
                            ));
                        }
                        Err(err) => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(err));
                        }
                    }
                }
            });
        });
    }

    {
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets.create_button.connect_clicked(move |_| {
            runtime.spawn({
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                async move {
                    let guard = app_runtime.lock().await;
                    match guard.create_meeting("meet", None) {
                        Ok(()) => {
                            let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                "Opened create-meeting URL".to_string(),
                            ));
                        }
                        Err(err) => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(err));
                        }
                    }
                }
            });
        });
    }

    {
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets.snooze_button.connect_clicked(move |_| {
            runtime.spawn({
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                async move {
                    let guard = app_runtime.lock().await;
                    match guard.snooze(10).await {
                        Ok(()) => {
                            let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                "Notifications snoozed for 10 minutes".to_string(),
                            ));
                        }
                        Err(err) => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(err));
                        }
                    }
                }
            });
        });
    }

    {
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets.calendar_button.connect_clicked(move |_| {
            runtime.spawn({
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                async move {
                    let guard = app_runtime.lock().await;
                    match guard.open_calendar_day() {
                        Ok(()) => {
                            let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                "Opened calendar day".to_string(),
                            ));
                        }
                        Err(err) => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(err));
                        }
                    }
                }
            });
        });
    }

    {
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets.clear_dismissals_button.connect_clicked(move |_| {
            runtime.spawn({
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                async move {
                    let mut guard = app_runtime.lock().await;
                    guard.clear_dismissals();
                    match guard.refresh().await {
                        Ok(_) => {
                            let meetings = guard.state.meetings().to_vec();
                            let _ = ui_tx.send(UiEvent::MeetingsLoaded(meetings));
                            let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                "Dismissals cleared".to_string(),
                            ));
                        }
                        Err(err) => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(err));
                        }
                    }
                }
            });
        });
    }
}

fn trigger_refresh(
    runtime: Arc<Runtime>,
    app_runtime: Arc<Mutex<AppRuntime>>,
    ui_tx: mpsc::Sender<UiEvent>,
) {
    runtime.spawn(async move {
        let mut guard = app_runtime.lock().await;

        match guard.force_refresh().await {
            Ok(()) => {
                let _ = ui_tx.send(UiEvent::ActionSucceeded(
                    "Daemon refresh requested".to_string(),
                ));
            }
            Err(err) => {
                let _ = ui_tx.send(UiEvent::ActionFailed(err));
            }
        }

        match guard.refresh().await {
            Ok(_) => {
                let meetings = guard.state.meetings().to_vec();
                let _ = ui_tx.send(UiEvent::MeetingsLoaded(meetings));
            }
            Err(err) => {
                guard.state.set_disconnected();
                let _ = ui_tx.send(UiEvent::ActionFailed(err));
            }
        }
    });
}

fn update_hero(widgets: &UiWidgets, meetings: &[nextmeeting_core::MeetingView]) {
    if let Some(meeting) = meetings.first() {
        let service = meeting
            .primary_link
            .as_ref()
            .map(|link| link.kind.display_name().to_string())
            .unwrap_or_else(|| "No link".to_string());

        let timing = if meeting.is_ongoing {
            format!(
                "Happening now • {}",
                format_time_range(meeting.start_local, meeting.end_local)
            )
        } else {
            format_time_range(meeting.start_local, meeting.end_local)
        };

        widgets.hero_title_label.set_label(&truncate(&meeting.title, 90));
        widgets.hero_meta_label.set_label(&timing);
        widgets.hero_service_label.set_label(&service);
    } else {
        widgets.hero_title_label.set_label("No upcoming meetings");
        widgets.hero_meta_label.set_label("Try Refresh to pull the latest events");
        widgets.hero_service_label.set_label("No link");
    }
}

fn render_meetings(
    widgets: &UiWidgets,
    meetings: &[nextmeeting_core::MeetingView],
    runtime: Arc<Runtime>,
    app_runtime: Arc<Mutex<AppRuntime>>,
    ui_tx: mpsc::Sender<UiEvent>,
) {
    while let Some(child) = widgets.listbox.first_child() {
        widgets.listbox.remove(&child);
    }

    if meetings.is_empty() {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("meeting-empty-row");
        let label = gtk::Label::builder()
            .label("No visible meetings")
            .css_classes(["meeting-empty-label"])
            .xalign(0.0)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();
        row.set_child(Some(&label));
        widgets.listbox.append(&row);
        return;
    }

    for meeting in meetings {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("meeting-row-container");

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 8);
        outer.add_css_class("meeting-row");
        outer.set_margin_top(8);
        outer.set_margin_bottom(8);
        outer.set_margin_start(8);
        outer.set_margin_end(8);

        let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text.set_hexpand(true);

        let title = gtk::Label::builder()
            .label(truncate(&meeting.title, 64))
            .xalign(0.0)
            .css_classes(["meeting-title"])
            .build();
        title.set_wrap(false);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);

        let time_line = if meeting.is_ongoing {
            format!(
                "Happening now • {}",
                format_time_range(meeting.start_local, meeting.end_local)
            )
        } else {
            format_time_range(meeting.start_local, meeting.end_local)
        };

        let when = gtk::Label::builder()
            .label(time_line)
            .xalign(0.0)
            .css_classes(["meeting-time"])
            .build();
        if meeting.is_ongoing {
            when.add_css_class("meeting-live");
        }

        let service = meeting
            .primary_link
            .as_ref()
            .map(|link| link.kind.display_name().to_string())
            .unwrap_or_else(|| "No link".to_string());

        let service_label = gtk::Label::builder()
            .label(service)
            .xalign(0.0)
            .css_classes(["meeting-service"])
            .build();

        text.append(&title);
        text.append(&when);
        text.append(&service_label);

        let dismiss = gtk::Button::builder()
            .label("Hide")
            .css_classes(["flat", "dismiss-button"])
            .build();
        let event_id = meeting.id.clone();

        {
            let runtime = runtime.clone();
            let app_runtime = app_runtime.clone();
            let ui_tx = ui_tx.clone();
            dismiss.connect_clicked(move |_| {
                runtime.spawn({
                    let app_runtime = app_runtime.clone();
                    let ui_tx = ui_tx.clone();
                    let event_id = event_id.clone();
                    async move {
                        let mut guard = app_runtime.lock().await;
                        guard.dismiss_event(&event_id);
                        let meetings = guard.state.meetings().to_vec();
                        let _ = ui_tx.send(UiEvent::MeetingsLoaded(meetings));
                        let _ = ui_tx.send(UiEvent::ActionSucceeded(format!(
                            "Dismissed event {event_id}"
                        )));
                    }
                });
            });
        }

        top.append(&text);
        top.append(&dismiss);
        outer.append(&top);

        row.set_child(Some(&outer));
        widgets.listbox.append(&row);
    }
}
