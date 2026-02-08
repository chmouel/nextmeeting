use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use nextmeeting_protocol::EventMutationAction;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use chrono::{DateTime, Local, Utc};

use crate::config::GtkConfig;
use crate::tray::{TrayCommand, manager::TrayManager};
use crate::widgets::meeting_card::MeetingCard;
use crate::widgets::window::{UiWidgets, build as build_window};

fn format_snooze_time(until: DateTime<Utc>) -> String {
    let local: DateTime<Local> = until.into();
    local.format("%H:%M").to_string()
}

#[derive(Debug)]
pub struct AppRuntime {
    config: GtkConfig,
    daemon: crate::daemon::client::DaemonClient,
    pub state: crate::daemon::state::MeetingState,
    dismissals: crate::dismissals::DismissedEvents,
    snooze_minutes: u32,
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
        let snooze_minutes = config.snooze_minutes();
        Self {
            config,
            daemon,
            state: crate::daemon::state::MeetingState::default(),
            dismissals: crate::dismissals::DismissedEvents::load(),
            snooze_minutes,
        }
    }

    pub fn snooze_minutes(&self) -> u32 {
        self.snooze_minutes
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

    pub async fn get_snoozed_until(&self) -> Option<DateTime<Utc>> {
        self.daemon
            .get_status()
            .await
            .ok()
            .and_then(|s| s.snoozed_until)
    }

    pub fn dismiss_event(&mut self, event_id: &str) {
        self.dismissals.dismiss(event_id.to_string());
        self.state.remove_meeting(event_id);
    }

    pub async fn mutate_event(
        &mut self,
        provider_name: &str,
        calendar_id: &str,
        event_id: &str,
        action: EventMutationAction,
    ) -> Result<(), String> {
        self.daemon
            .mutate_event(provider_name, calendar_id, event_id, action)
            .await?;
        self.state
            .remove_meeting_exact(provider_name, calendar_id, event_id);
        Ok(())
    }

    pub fn clear_dismissals(&mut self) {
        self.dismissals.clear();
    }

    pub fn open_next_meeting(&self) -> Result<(), String> {
        nextmeeting_client::actions::open_meeting_url(self.state.meetings())
            .map_err(|e| e.to_string())
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
    SnoozeStateChanged(Option<DateTime<Utc>>),
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
            if let Some(window) = app.windows().into_iter().next() {
                window.present();
                return;
            }
            build_ui(app, runtime.clone(), app_runtime.clone());
        });

        app.run()
    }
}

fn build_ui(app: &adw::Application, runtime: Arc<Runtime>, app_runtime: Arc<Mutex<AppRuntime>>) {
    // Get snooze_minutes synchronously at startup
    let snooze_minutes = runtime.block_on(async {
        let guard = app_runtime.lock().await;
        guard.snooze_minutes()
    });
    let widgets = Rc::new(build_window(app, snooze_minutes));
    widgets.window.set_hide_on_close(true);

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
                        render_meetings(
                            &widgets_for_events,
                            &meetings,
                            runtime_for_events.clone(),
                            app_runtime_for_events.clone(),
                            ui_tx_for_events.clone(),
                        );
                    }
                    UiEvent::ActionFailed(err) => {
                        let toast = adw::Toast::builder().title(&err).timeout(3).build();
                        widgets_for_events.toast_overlay.add_toast(toast);
                    }
                    UiEvent::ActionSucceeded(msg) => {
                        let toast = adw::Toast::builder().title(&msg).timeout(2).build();
                        widgets_for_events.toast_overlay.add_toast(toast);
                    }
                    UiEvent::SnoozeStateChanged(snoozed_until) => {
                        update_snooze_button(
                            &widgets_for_events.snooze_button,
                            snoozed_until,
                            snooze_minutes,
                        );
                    }
                }
            }

            glib::ControlFlow::Continue
        });
    }

    connect_actions(
        &widgets,
        runtime.clone(),
        app_runtime.clone(),
        ui_tx.clone(),
    );

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

    // Periodic snooze status check (every 30 seconds)
    {
        let runtime_for_snooze = runtime.clone();
        let app_runtime_for_snooze = app_runtime.clone();
        let ui_tx_for_snooze = ui_tx.clone();

        glib::source::timeout_add_seconds_local(30, move || {
            let runtime = runtime_for_snooze.clone();
            let app_runtime = app_runtime_for_snooze.clone();
            let ui_tx = ui_tx_for_snooze.clone();

            runtime.spawn(async move {
                let guard = app_runtime.lock().await;
                let snoozed_until = guard.get_snoozed_until().await;
                let _ = ui_tx.send(UiEvent::SnoozeStateChanged(snoozed_until));
            });

            glib::ControlFlow::Continue
        });
    }

    trigger_refresh(runtime.clone(), app_runtime.clone(), ui_tx.clone());

    // Initial snooze state check
    {
        let app_runtime_init = app_runtime.clone();
        let ui_tx_init = ui_tx;

        runtime.spawn(async move {
            let guard = app_runtime_init.lock().await;
            let snoozed_until = guard.get_snoozed_until().await;
            let _ = ui_tx_init.send(UiEvent::SnoozeStateChanged(snoozed_until));
        });
    }

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
                    // Check if already snoozed - if so, clear it (toggle behavior)
                    let currently_snoozed = guard.get_snoozed_until().await;
                    let is_active = currently_snoozed
                        .map(|until| until > Utc::now())
                        .unwrap_or(false);

                    let minutes = if is_active { 0 } else { guard.snooze_minutes() };
                    match guard.snooze(minutes).await {
                        Ok(()) => {
                            if is_active {
                                let _ = ui_tx
                                    .send(UiEvent::ActionSucceeded("Snooze cleared".to_string()));
                                let _ = ui_tx.send(UiEvent::SnoozeStateChanged(None));
                            } else {
                                let _ = ui_tx.send(UiEvent::ActionSucceeded(format!(
                                    "Snoozed {} min",
                                    minutes
                                )));
                                // Query the snoozed_until from server to update UI
                                let snoozed_until = guard.get_snoozed_until().await;
                                let _ = ui_tx.send(UiEvent::SnoozeStateChanged(snoozed_until));
                            }
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
                            let _ =
                                ui_tx.send(UiEvent::ActionSucceeded("Opened calendar".to_string()));
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
                            let _ = ui_tx
                                .send(UiEvent::ActionSucceeded("Dismissals cleared".to_string()));
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
                let _ = ui_tx.send(UiEvent::ActionSucceeded("Refreshing…".to_string()));
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

fn render_meetings(
    widgets: &UiWidgets,
    meetings: &[nextmeeting_core::MeetingView],
    runtime: Arc<Runtime>,
    app_runtime: Arc<Mutex<AppRuntime>>,
    ui_tx: mpsc::Sender<UiEvent>,
) {
    // Clear existing cards
    while let Some(child) = widgets.meeting_cards_container.first_child() {
        widgets.meeting_cards_container.remove(&child);
    }

    if meetings.is_empty() {
        // Empty state
        let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        empty_box.set_halign(gtk::Align::Center);
        empty_box.set_valign(gtk::Align::Center);
        empty_box.set_margin_top(40);
        empty_box.set_margin_bottom(40);

        let icon = gtk::Image::from_icon_name("x-office-calendar-symbolic");
        icon.set_pixel_size(64);
        icon.add_css_class("empty-state-icon");

        let label = gtk::Label::builder()
            .label("No meetings today")
            .css_classes(["empty-state-label"])
            .build();

        empty_box.append(&icon);
        empty_box.append(&label);
        widgets.meeting_cards_container.append(&empty_box);
        return;
    }

    // Find the first current/ongoing meeting for the JOIN NOW button
    let current_meeting_id = find_current_meeting(meetings);

    for (index, meeting) in meetings.iter().enumerate() {
        let show_join = current_meeting_id.as_ref() == Some(&meeting.id);
        let always_show_actions = index == 0;
        let card = MeetingCard::new(meeting, show_join, always_show_actions);

        // Connect join button if present
        if let Some(ref join_btn) = card.join_button {
            let runtime = runtime.clone();
            let app_runtime = app_runtime.clone();
            let ui_tx = ui_tx.clone();
            join_btn.connect_clicked(move |_| {
                runtime.spawn({
                    let app_runtime = app_runtime.clone();
                    let ui_tx = ui_tx.clone();
                    async move {
                        let guard = app_runtime.lock().await;
                        match guard.open_next_meeting() {
                            Ok(()) => {
                                let _ = ui_tx
                                    .send(UiEvent::ActionSucceeded("Opened meeting".to_string()));
                            }
                            Err(err) => {
                                let _ = ui_tx.send(UiEvent::ActionFailed(err));
                            }
                        }
                    }
                });
            });
        }

        // Connect dismiss button
        {
            let runtime = runtime.clone();
            let app_runtime = app_runtime.clone();
            let ui_tx = ui_tx.clone();
            let event_id = meeting.id.clone();
            card.dismiss_button.connect_clicked(move |_| {
                let runtime = runtime.clone();
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                let event_id = event_id.clone();
                runtime.spawn(async move {
                    let mut guard = app_runtime.lock().await;
                    guard.dismiss_event(&event_id);
                    let meetings = guard.state.meetings().to_vec();
                    let _ = ui_tx.send(UiEvent::MeetingsLoaded(meetings));
                    let _ = ui_tx.send(UiEvent::ActionSucceeded("Event dismissed".to_string()));
                });
            });
        }

        // Connect decline button
        {
            let runtime = runtime.clone();
            let app_runtime = app_runtime.clone();
            let ui_tx = ui_tx.clone();
            let provider_name = meeting.provider_name.clone();
            let calendar_id = meeting.calendar_id.clone();
            let event_id = meeting.id.clone();
            card.decline_button.connect_clicked(move |_| {
                let runtime = runtime.clone();
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                let provider_name = provider_name.clone();
                let calendar_id = calendar_id.clone();
                let event_id = event_id.clone();
                runtime.spawn(async move {
                    let mut guard = app_runtime.lock().await;
                    match guard
                        .mutate_event(
                            &provider_name,
                            &calendar_id,
                            &event_id,
                            EventMutationAction::Decline,
                        )
                        .await
                    {
                        Ok(()) => {
                            let meetings = guard.state.meetings().to_vec();
                            let _ = ui_tx.send(UiEvent::MeetingsLoaded(meetings));
                            let _ =
                                ui_tx.send(UiEvent::ActionSucceeded("Event declined".to_string()));
                        }
                        Err(err) => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(err));
                        }
                    }
                });
            });
        }

        // Connect delete button (with confirmation dialog)
        {
            let runtime = runtime.clone();
            let app_runtime = app_runtime.clone();
            let ui_tx = ui_tx.clone();
            let provider_name = meeting.provider_name.clone();
            let calendar_id = meeting.calendar_id.clone();
            let event_id = meeting.id.clone();
            let event_title = meeting.title.clone();
            let window = widgets.window.clone();
            card.delete_button.connect_clicked(move |_| {
                let runtime = runtime.clone();
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                let provider_name = provider_name.clone();
                let calendar_id = calendar_id.clone();
                let event_id = event_id.clone();
                let event_title = event_title.clone();
                let window = window.clone();

                // Create confirmation dialog
                let dialog = adw::AlertDialog::builder()
                    .heading("Delete Event?")
                    .body(format!(
                        "Are you sure you want to delete \"{}\"? This cannot be undone.",
                        event_title
                    ))
                    .default_response("cancel")
                    .close_response("cancel")
                    .build();
                dialog.add_response("cancel", "Cancel");
                dialog.add_response("delete", "Delete");
                dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

                dialog.connect_response(None, move |_, response| {
                    if response == "delete" {
                        let runtime = runtime.clone();
                        let app_runtime = app_runtime.clone();
                        let ui_tx = ui_tx.clone();
                        let provider_name = provider_name.clone();
                        let calendar_id = calendar_id.clone();
                        let event_id = event_id.clone();
                        runtime.spawn(async move {
                            let mut guard = app_runtime.lock().await;
                            match guard
                                .mutate_event(
                                    &provider_name,
                                    &calendar_id,
                                    &event_id,
                                    EventMutationAction::Delete,
                                )
                                .await
                            {
                                Ok(()) => {
                                    let meetings = guard.state.meetings().to_vec();
                                    let _ = ui_tx.send(UiEvent::MeetingsLoaded(meetings));
                                    let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                        "Event deleted".to_string(),
                                    ));
                                }
                                Err(err) => {
                                    let _ = ui_tx.send(UiEvent::ActionFailed(err));
                                }
                            }
                        });
                    }
                });

                dialog.present(Some(&window));
            });
        }

        widgets.meeting_cards_container.append(&card.widget);
    }
}

/// Find the ID of the current/ongoing meeting that should get the JOIN NOW button.
/// Priority: ongoing meetings first, then meetings starting within 5 minutes.
fn find_current_meeting(meetings: &[nextmeeting_core::MeetingView]) -> Option<String> {
    // First check for ongoing meetings
    if let Some(m) = meetings.iter().find(|m| m.is_ongoing) {
        return Some(m.id.clone());
    }

    // Check for meetings starting within 5 minutes
    let now = chrono::Local::now();
    for meeting in meetings {
        let mins_until = meeting.minutes_until_start(now);
        if (0..=5).contains(&mins_until) {
            return Some(meeting.id.clone());
        }
    }

    // Return first meeting with a link as fallback
    meetings
        .iter()
        .find(|m| m.primary_link.is_some())
        .map(|m| m.id.clone())
}

fn update_snooze_button(
    button: &gtk::Button,
    snoozed_until: Option<DateTime<Utc>>,
    snooze_minutes: u32,
) {
    let now = Utc::now();
    match snoozed_until {
        Some(until) if until > now => {
            button.add_css_class("snoozed");
            button.set_tooltip_text(Some(&format!(
                "Snoozed until {}",
                format_snooze_time(until)
            )));
        }
        _ => {
            button.remove_css_class("snoozed");
            button.set_tooltip_text(Some(&format!(
                "Hide notifications for {} minutes",
                snooze_minutes
            )));
        }
    }
}
