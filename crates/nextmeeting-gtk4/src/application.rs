use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use nextmeeting_protocol::EventMutationAction;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use crate::config::GtkConfig;
use crate::tray::{TrayCommand, manager::TrayManager};
use crate::widgets::meeting_card::MeetingCard;
use crate::widgets::window::{
    LABEL_CALENDAR, LABEL_CLEAR_DISMISSALS, LABEL_CREATE_MEET, LABEL_REFRESH, LABEL_SNOOZE,
    UiWidgets, build as build_window,
};
use crate::window_state::WindowState;

#[derive(Debug)]
pub struct AppRuntime {
    config: GtkConfig,
    daemon: crate::daemon::client::DaemonClient,
    pub state: crate::daemon::state::MeetingState,
    dismissals: crate::dismissals::DismissedEvents,
    pub window_state: WindowState,
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
            window_state: WindowState::load(),
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
    let widgets = Rc::new(build_window(app));
    widgets.window.set_hide_on_close(true);

    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("../resources/style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk4::prelude::RootExt::display(&widgets.window),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Apply initial sidebar state - read synchronously before idle callback to avoid race
    let initial_collapsed = {
        let guard = runtime.block_on(app_runtime.lock());
        guard.window_state.is_sidebar_collapsed()
    };
    {
        let widgets = widgets.clone();
        glib::idle_add_local_once(move || {
            apply_sidebar_state(&widgets, initial_collapsed);
            widgets.sidebar_toggle_button.set_active(!initial_collapsed);
        });
    }

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
                    UiEvent::ActionFailed(_err) => {
                        // Error handling - could add toast notification here
                    }
                    UiEvent::ActionSucceeded(_msg) => {
                        // Success handling - could add toast notification here
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
                                "Snoozed 10 min".to_string(),
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
                            let _ = ui_tx
                                .send(UiEvent::ActionSucceeded("Opened calendar".to_string()));
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

    // Sidebar toggle button
    {
        let toggle_button = widgets.sidebar_toggle_button.clone();
        let widgets = widgets.clone();
        let runtime = runtime.clone();
        let app_runtime = app_runtime.clone();
        toggle_button.connect_toggled(move |btn| {
            let collapsed = !btn.is_active();
            apply_sidebar_state(&widgets, collapsed);

            // Persist state
            let app_runtime = app_runtime.clone();
            runtime.spawn(async move {
                let mut guard = app_runtime.lock().await;
                guard.window_state.set_sidebar_collapsed(collapsed);
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
                    "Refreshing…".to_string(),
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

    for meeting in meetings {
        let show_join = current_meeting_id.as_ref() == Some(&meeting.id);
        let card = MeetingCard::new(meeting, show_join);

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
                                let _ = ui_tx.send(UiEvent::ActionSucceeded(
                                    "Opened meeting".to_string(),
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
        if mins_until >= 0 && mins_until <= 5 {
            return Some(meeting.id.clone());
        }
    }

    // Return first meeting with a link as fallback
    meetings
        .iter()
        .find(|m| m.primary_link.is_some())
        .map(|m| m.id.clone())
}

fn apply_sidebar_state(widgets: &UiWidgets, collapsed: bool) {
    if collapsed {
        widgets.left_sidebar.add_css_class("left-sidebar-collapsed");
        widgets.left_sidebar.set_width_request(64);

        // Update toggle button icon and tooltip
        widgets
            .sidebar_toggle_button
            .set_icon_name("sidebar-show-symbolic");
        widgets
            .sidebar_toggle_button
            .set_tooltip_text(Some("Show Sidebar"));

        // Hide button labels
        set_button_label(&widgets.create_button, "");
        set_button_label(&widgets.snooze_button, "");
        set_button_label(&widgets.calendar_button, "");
        set_button_label(&widgets.refresh_button, "");
        set_button_label(&widgets.clear_dismissals_button, "");
    } else {
        widgets
            .left_sidebar
            .remove_css_class("left-sidebar-collapsed");
        widgets.left_sidebar.set_width_request(220);

        // Update toggle button icon and tooltip
        widgets
            .sidebar_toggle_button
            .set_icon_name("sidebar-hide-symbolic");
        widgets
            .sidebar_toggle_button
            .set_tooltip_text(Some("Hide Sidebar"));

        // Restore button labels
        set_button_label(&widgets.create_button, LABEL_CREATE_MEET);
        set_button_label(&widgets.snooze_button, LABEL_SNOOZE);
        set_button_label(&widgets.calendar_button, LABEL_CALENDAR);
        set_button_label(&widgets.refresh_button, LABEL_REFRESH);
        set_button_label(&widgets.clear_dismissals_button, LABEL_CLEAR_DISMISSALS);
    }
}

fn set_button_label(button: &gtk::Button, label: &str) {
    if let Some(child) = button.child() {
        if let Some(btn_content) = child.downcast_ref::<adw::ButtonContent>() {
            btn_content.set_label(label);
        }
    }
}
