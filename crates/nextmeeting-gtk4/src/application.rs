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
    show_dismissed: bool,
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
            show_dismissed: false,
        }
    }

    pub fn snooze_minutes(&self) -> u32 {
        self.snooze_minutes
    }

    pub async fn refresh(&mut self) -> Result<usize, String> {
        let meetings = self.daemon.get_meetings().await?;
        let visible: Vec<_> = if self.show_dismissed {
            meetings
        } else {
            meetings
                .into_iter()
                .filter(|meeting| !self.dismissals.is_dismissed(&meeting.id))
                .collect()
        };
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

    pub fn toggle_show_dismissed(&mut self) -> bool {
        self.show_dismissed = !self.show_dismissed;
        self.show_dismissed
    }

    pub fn show_dismissed(&self) -> bool {
        self.show_dismissed
    }

    pub fn undismiss_event(&mut self, event_id: &str) {
        self.dismissals.undismiss(event_id);
    }

    pub fn dismissed_ids(&self) -> &std::collections::HashSet<String> {
        self.dismissals.dismissed_ids()
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

    pub fn edit_calendar_event_url(&self, url: &str, event_id: &str) -> Result<(), String> {
        let domain = self.config.client.google_domain.as_deref();
        nextmeeting_client::actions::edit_calendar_event_url(url, event_id, domain)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug)]
pub struct GtkApp {
    runtime: Arc<Runtime>,
    app_runtime: Arc<Mutex<AppRuntime>>,
}

use std::collections::HashSet;

#[derive(Debug)]
enum UiEvent {
    MeetingsLoaded {
        meetings: Vec<nextmeeting_core::MeetingView>,
        show_dismissed: bool,
        dismissed_ids: HashSet<String>,
    },
    ActionFailed(String),
    ActionSucceeded(String),
    SnoozeStateChanged(Option<DateTime<Utc>>),
    ShowDismissedChanged(bool),
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
                    UiEvent::MeetingsLoaded {
                        meetings,
                        show_dismissed,
                        dismissed_ids,
                    } => {
                        render_meetings(
                            &widgets_for_events,
                            &meetings,
                            show_dismissed,
                            &dismissed_ids,
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
                    UiEvent::ShowDismissedChanged(showing) => {
                        update_show_dismissed_button(
                            &widgets_for_events.clear_dismissals_button,
                            showing,
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

    // Ctrl+Q quit with confirmation dialog
    {
        let quit_action = gtk::gio::SimpleAction::new("quit-confirmed", None);
        let window = widgets.window.clone();
        let app_for_quit = app.clone();
        quit_action.connect_activate(move |_, _| {
            let dialog = adw::AlertDialog::builder()
                .heading("Quit NextMeeting?")
                .body("The application will stop running in the background.")
                .default_response("cancel")
                .close_response("cancel")
                .build();
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("quit", "Quit");
            dialog.set_response_appearance("quit", adw::ResponseAppearance::Destructive);

            let app_clone = app_for_quit.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "quit" {
                    app_clone.quit();
                }
            });

            dialog.present(Some(&window));
        });
        app.add_action(&quit_action);
        app.set_accels_for_action("app.quit-confirmed", &["<Control>q"]);
    }

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
                    let showing = guard.toggle_show_dismissed();
                    let _ = ui_tx.send(UiEvent::ShowDismissedChanged(showing));
                    match guard.refresh().await {
                        Ok(_) => {
                            let meetings = guard.state.meetings().to_vec();
                            let show_dismissed = guard.show_dismissed();
                            let dismissed_ids = guard.dismissed_ids().clone();
                            let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                                meetings,
                                show_dismissed,
                                dismissed_ids,
                            });
                            let msg = if showing {
                                "Showing dismissed meetings"
                            } else {
                                "Hiding dismissed meetings"
                            };
                            let _ = ui_tx.send(UiEvent::ActionSucceeded(msg.to_string()));
                        }
                        Err(err) => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(err));
                        }
                    }
                }
            });
        });
    }

    // === Detail panel buttons (connected once) ===

    // Close button
    {
        let detail_panel = widgets.detail_panel.clone();
        widgets
            .detail_panel
            .close_button()
            .connect_clicked(move |_| {
                detail_panel.hide();
            });
    }

    // Join button
    {
        let action_ctx = widgets.detail_panel.action_context();
        let ui_tx = ui_tx.clone();
        widgets
            .detail_panel
            .join_button()
            .connect_clicked(move |_| {
                let ctx = action_ctx.borrow();
                if let Some(ref ctx) = *ctx
                    && let Some(ref url) = ctx.primary_link_url {
                        match open::that(url) {
                            Ok(()) => {
                                let _ = ui_tx
                                    .send(UiEvent::ActionSucceeded("Opened meeting".to_string()));
                            }
                            Err(err) => {
                                let _ = ui_tx.send(UiEvent::ActionFailed(err.to_string()));
                            }
                        }
                    }
            });
    }

    // Dismiss button
    {
        let action_ctx = widgets.detail_panel.action_context();
        let detail_panel = widgets.detail_panel.clone();
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets
            .detail_panel
            .dismiss_button()
            .connect_clicked(move |_| {
                let ctx = action_ctx.borrow().clone();
                if let Some(ctx) = ctx {
                    let runtime = runtime.clone();
                    let app_runtime = app_runtime.clone();
                    let ui_tx = ui_tx.clone();
                    let detail_panel = detail_panel.clone();
                    runtime.spawn(async move {
                        let mut guard = app_runtime.lock().await;
                        if ctx.is_dismissed {
                            guard.undismiss_event(&ctx.event_id);
                        } else {
                            guard.dismiss_event(&ctx.event_id);
                        }
                        if guard.refresh().await.is_ok() {
                            let meetings = guard.state.meetings().to_vec();
                            let show_dismissed = guard.show_dismissed();
                            let dismissed_ids = guard.dismissed_ids().clone();
                            let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                                meetings,
                                show_dismissed,
                                dismissed_ids,
                            });
                        }
                        let msg = if ctx.is_dismissed {
                            "Event restored"
                        } else {
                            "Event dismissed"
                        };
                        let _ = ui_tx.send(UiEvent::ActionSucceeded(msg.to_string()));
                    });
                    // Hide panel immediately on the GTK thread
                    detail_panel.hide();
                }
            });
    }

    // Decline button
    {
        let action_ctx = widgets.detail_panel.action_context();
        let detail_panel = widgets.detail_panel.clone();
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets
            .detail_panel
            .decline_button()
            .connect_clicked(move |_| {
                let ctx = action_ctx.borrow().clone();
                if let Some(ctx) = ctx {
                    let runtime = runtime.clone();
                    let app_runtime = app_runtime.clone();
                    let ui_tx = ui_tx.clone();
                    runtime.spawn(async move {
                        let mut guard = app_runtime.lock().await;
                        match guard
                            .mutate_event(
                                &ctx.provider_name,
                                &ctx.calendar_id,
                                &ctx.event_id,
                                EventMutationAction::Decline,
                            )
                            .await
                        {
                            Ok(()) => {
                                let meetings = guard.state.meetings().to_vec();
                                let show_dismissed = guard.show_dismissed();
                                let dismissed_ids = guard.dismissed_ids().clone();
                                let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                                    meetings,
                                    show_dismissed,
                                    dismissed_ids,
                                });
                                let _ = ui_tx
                                    .send(UiEvent::ActionSucceeded("Event declined".to_string()));
                            }
                            Err(err) => {
                                let _ = ui_tx.send(UiEvent::ActionFailed(err));
                            }
                        }
                    });
                    detail_panel.hide();
                }
            });
    }

    // Delete button (with confirmation)
    {
        let action_ctx = widgets.detail_panel.action_context();
        let detail_panel = widgets.detail_panel.clone();
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        let window = widgets.window.clone();
        widgets
            .detail_panel
            .delete_button()
            .connect_clicked(move |_| {
                let ctx = action_ctx.borrow().clone();
                if let Some(ctx) = ctx {
                    let runtime = runtime.clone();
                    let app_runtime = app_runtime.clone();
                    let ui_tx = ui_tx.clone();
                    let detail_panel = detail_panel.clone();

                    let dialog = adw::AlertDialog::builder()
                        .heading("Delete Event?")
                        .body(format!(
                            "Are you sure you want to delete \"{}\"? This cannot be undone.",
                            ctx.title
                        ))
                        .default_response("cancel")
                        .close_response("cancel")
                        .build();
                    dialog.add_response("cancel", "Cancel");
                    dialog.add_response("delete", "Delete");
                    dialog
                        .set_response_appearance("delete", adw::ResponseAppearance::Destructive);

                    dialog.connect_response(None, move |_, response| {
                        if response == "delete" {
                            let runtime = runtime.clone();
                            let app_runtime = app_runtime.clone();
                            let ui_tx = ui_tx.clone();
                            let ctx = ctx.clone();
                            runtime.spawn(async move {
                                let mut guard = app_runtime.lock().await;
                                match guard
                                    .mutate_event(
                                        &ctx.provider_name,
                                        &ctx.calendar_id,
                                        &ctx.event_id,
                                        EventMutationAction::Delete,
                                    )
                                    .await
                                {
                                    Ok(()) => {
                                        let meetings = guard.state.meetings().to_vec();
                                        let show_dismissed = guard.show_dismissed();
                                        let dismissed_ids = guard.dismissed_ids().clone();
                                        let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                                            meetings,
                                            show_dismissed,
                                            dismissed_ids,
                                        });
                                        let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                            "Event deleted".to_string(),
                                        ));
                                    }
                                    Err(err) => {
                                        let _ = ui_tx.send(UiEvent::ActionFailed(err));
                                    }
                                }
                            });
                            detail_panel.hide();
                        }
                    });

                    dialog.present(Some(&window));
                }
            });
    }

    // Calendar button
    {
        let action_ctx = widgets.detail_panel.action_context();
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        let ui_tx = ui_tx.clone();
        widgets
            .detail_panel
            .calendar_button()
            .connect_clicked(move |_| {
                let ctx = action_ctx.borrow().clone();
                if let Some(ctx) = ctx {
                    let runtime = runtime.clone();
                    let app_runtime = app_runtime.clone();
                    let ui_tx = ui_tx.clone();
                    runtime.spawn(async move {
                        match ctx.calendar_url {
                            Some(url) => {
                                let guard = app_runtime.lock().await;
                                match guard.edit_calendar_event_url(&url, &ctx.event_id) {
                                    Ok(()) => {
                                        let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                            "Opened calendar event editor".to_string(),
                                        ));
                                    }
                                    Err(err) => {
                                        let _ = ui_tx.send(UiEvent::ActionFailed(err));
                                    }
                                }
                            }
                            None => {
                                let _ = ui_tx.send(UiEvent::ActionFailed(
                                    "No calendar event URL for this meeting".to_string(),
                                ));
                            }
                        }
                    });
                }
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
                let show_dismissed = guard.show_dismissed();
                let dismissed_ids = guard.dismissed_ids().clone();
                let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                    meetings,
                    show_dismissed,
                    dismissed_ids,
                });
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
    _show_dismissed: bool,
    dismissed_ids: &HashSet<String>,
    runtime: Arc<Runtime>,
    app_runtime: Arc<Mutex<AppRuntime>>,
    ui_tx: mpsc::Sender<UiEvent>,
) {
    // Clear existing cards and hide detail panel on refresh
    while let Some(child) = widgets.meeting_cards_container.first_child() {
        widgets.meeting_cards_container.remove(&child);
    }
    widgets.detail_panel.hide();

    if meetings.is_empty() {
        // Empty state
        let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        empty_box.set_halign(gtk::Align::Center);
        empty_box.set_valign(gtk::Align::Center);
        empty_box.set_margin_top(40);
        empty_box.set_margin_bottom(40);

        let icon = gtk::Image::from_icon_name("calendar-month-symbolic");
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

    // Find meetings that should get the JOIN NOW button
    let primary_ids = find_primary_meetings(meetings);

    const SOON_THRESHOLD_MINUTES: i64 = 5;
    let now = Local::now();

    for meeting in meetings {
        let is_primary = primary_ids.contains(&meeting.id);
        let has_link = meeting.primary_link.is_some();
        let always_show_actions = is_primary;
        let is_dismissed = dismissed_ids.contains(&meeting.id);
        let minutes_until = meeting.minutes_until_start(now);
        let is_ongoing = meeting.start_local <= now && now < meeting.end_local;
        let is_soon = !is_ongoing
            && !meeting.is_all_day
            && minutes_until > 0
            && minutes_until <= SOON_THRESHOLD_MINUTES;
        let show_join_button = is_primary && has_link;
        let show_card_actions = is_primary;
        let card = MeetingCard::new(
            meeting,
            show_join_button,
            is_primary,
            always_show_actions,
            is_dismissed,
            is_soon,
            show_card_actions,
        );

        // Click handler: toggle detail panel
        {
            let card_widget = card.widget.clone();
            let detail_panel = widgets.detail_panel.clone();
            let meeting_clone = meeting.clone();
            let card_is_dismissed = is_dismissed;
            let click = gtk::GestureClick::new();
            click.set_button(gtk::gdk::BUTTON_PRIMARY);
            click.connect_released(move |gesture, _, x, y| {
                if gesture.current_button() != gtk::gdk::BUTTON_PRIMARY {
                    return;
                }

                if let Some(picked_widget) = card_widget.pick(x, y, gtk::PickFlags::DEFAULT)
                    && widget_or_ancestor_has_css_class(
                        &picked_widget,
                        "meeting-card-interactive-action",
                    )
                {
                    return;
                }

                // Toggle: if already showing this meeting, hide; otherwise show
                if detail_panel.current_meeting_id().as_ref() == Some(&meeting_clone.id) {
                    detail_panel.hide();
                } else {
                    detail_panel.show_meeting(&meeting_clone, card_is_dismissed);
                }
            });
            card.widget.add_controller(click);
        }

        // Connect join button if present
        if let Some(ref join_btn) = card.join_button
            && let Some(ref link) = meeting.primary_link {
                let url = link.url.clone();
                let ui_tx = ui_tx.clone();
                join_btn.connect_clicked(move |_| {
                    match open::that(&url) {
                        Ok(()) => {
                            let _ = ui_tx
                                .send(UiEvent::ActionSucceeded("Opened meeting".to_string()));
                        }
                        Err(err) => {
                            let _ = ui_tx
                                .send(UiEvent::ActionFailed(err.to_string()));
                        }
                    }
                });
            }

        // Connect dismiss button (toggles between dismiss and undismiss)
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
                    let was_dismissed = guard.dismissed_ids().contains(&event_id);
                    if was_dismissed {
                        guard.undismiss_event(&event_id);
                    } else {
                        guard.dismiss_event(&event_id);
                    }
                    // Refresh to get the updated meeting list
                    if guard.refresh().await.is_ok() {
                        let meetings = guard.state.meetings().to_vec();
                        let show_dismissed = guard.show_dismissed();
                        let dismissed_ids = guard.dismissed_ids().clone();
                        let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                            meetings,
                            show_dismissed,
                            dismissed_ids,
                        });
                    }
                    let msg = if was_dismissed {
                        "Event restored"
                    } else {
                        "Event dismissed"
                    };
                    let _ = ui_tx.send(UiEvent::ActionSucceeded(msg.to_string()));
                });
            });
        }

        // Connect edit-calendar-event button
        {
            let runtime = runtime.clone();
            let app_runtime = app_runtime.clone();
            let ui_tx = ui_tx.clone();
            let calendar_url = meeting.calendar_url.clone();
            let event_id_for_edit = meeting.id.clone();
            card.calendar_button.connect_clicked(move |_| {
                let runtime = runtime.clone();
                let app_runtime = app_runtime.clone();
                let ui_tx = ui_tx.clone();
                let calendar_url = calendar_url.clone();
                let event_id = event_id_for_edit.clone();
                runtime.spawn(async move {
                    match calendar_url {
                        Some(url) => {
                            let guard = app_runtime.lock().await;
                            match guard.edit_calendar_event_url(&url, &event_id) {
                                Ok(()) => {
                                    let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                        "Opened calendar event editor".to_string(),
                                    ));
                                }
                                Err(err) => {
                                    let _ = ui_tx.send(UiEvent::ActionFailed(err));
                                }
                            }
                        }
                        None => {
                            let _ = ui_tx.send(UiEvent::ActionFailed(
                                "No calendar event URL for this meeting".to_string(),
                            ));
                        }
                    }
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
                            let show_dismissed = guard.show_dismissed();
                            let dismissed_ids = guard.dismissed_ids().clone();
                            let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                                meetings,
                                show_dismissed,
                                dismissed_ids,
                            });
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
                                    let show_dismissed = guard.show_dismissed();
                                    let dismissed_ids = guard.dismissed_ids().clone();
                                    let _ = ui_tx.send(UiEvent::MeetingsLoaded {
                                        meetings,
                                        show_dismissed,
                                        dismissed_ids,
                                    });
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

/// Find the IDs of meetings that should get the JOIN NOW button.
/// Priority: all ongoing with link > imminent (within 5 min) > next upcoming with link > first with link.
fn find_primary_meetings(meetings: &[nextmeeting_core::MeetingView]) -> HashSet<String> {
    let now = chrono::Local::now();

    // First: all ongoing meetings with a link
    let ongoing: HashSet<String> = meetings
        .iter()
        .filter(|m| m.start_local <= now && now < m.end_local && m.primary_link.is_some())
        .map(|m| m.id.clone())
        .collect();
    if !ongoing.is_empty() {
        return ongoing;
    }

    // Second: imminent meeting starting within 5 minutes (not yet started)
    for meeting in meetings {
        let mins_until = meeting.minutes_until_start(now);
        if meeting.start_local > now && (0..=5).contains(&mins_until) && meeting.primary_link.is_some() {
            return HashSet::from([meeting.id.clone()]);
        }
    }

    // Third: next upcoming meeting with a link (hasn't started yet, soonest first)
    if let Some(m) = meetings
        .iter()
        .find(|m| m.start_local > now && m.primary_link.is_some())
    {
        return HashSet::from([m.id.clone()]);
    }

    // Last resort: first meeting with a link
    meetings
        .iter()
        .find(|m| m.primary_link.is_some())
        .map(|m| HashSet::from([m.id.clone()]))
        .unwrap_or_default()
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

fn update_show_dismissed_button(button: &gtk::Button, showing: bool) {
    if showing {
        button.add_css_class("showing-dismissed");
        button.set_tooltip_text(Some("Hide dismissed meetings"));
    } else {
        button.remove_css_class("showing-dismissed");
        button.set_tooltip_text(Some("Show dismissed meetings"));
    }
}

fn widget_or_ancestor_has_css_class(widget: &gtk::Widget, class_name: &str) -> bool {
    let mut current = Some(widget.clone());
    while let Some(item) = current {
        if item.has_css_class(class_name) {
            return true;
        }
        current = item.parent();
    }
    false
}
