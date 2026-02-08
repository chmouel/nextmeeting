use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::widgets::analog_clock;

// Button label constants - used in both window.rs and application.rs
pub const LABEL_CREATE_MEET: &str = "Create Meet";
pub const LABEL_SNOOZE: &str = "Snooze 10m";
pub const LABEL_CALENDAR: &str = "Calendar";
pub const LABEL_REFRESH: &str = "Refresh";
pub const LABEL_CLEAR_DISMISSALS: &str = "Clear Dismissals";

#[derive(Clone)]
pub struct UiWidgets {
    pub window: adw::ApplicationWindow,
    pub meeting_cards_container: gtk::Box,
    pub create_button: gtk::Button,
    pub refresh_button: gtk::Button,
    pub snooze_button: gtk::Button,
    pub calendar_button: gtk::Button,
    pub clear_dismissals_button: gtk::Button,
    pub sidebar_toggle_button: gtk::ToggleButton,
    pub left_sidebar: gtk::Box,
}

pub fn build(app: &adw::Application) -> UiWidgets {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("NextMeeting")
        .default_width(700)
        .default_height(640)
        .build();
    window.add_css_class("nm-window");

    let header = adw::HeaderBar::new();
    let window_title = adw::WindowTitle::builder()
        .title("NextMeeting")
        .subtitle("Today")
        .build();
    header.set_title_widget(Some(&window_title));

    // Sidebar toggle button
    let sidebar_toggle_button = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle Sidebar")
        .active(true) // Expanded by default
        .build();
    header.pack_start(&sidebar_toggle_button);

    // ===== LEFT COLUMN =====
    let left_column = gtk::Box::new(gtk::Orientation::Vertical, 16);
    left_column.add_css_class("left-column");
    left_column.set_hexpand(true);

    // Header section with title and clock
    let header_section = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    header_section.set_valign(gtk::Align::Start);

    let schedule_title = gtk::Label::builder()
        .label("Today's Schedule")
        .xalign(0.0)
        .hexpand(true)
        .css_classes(["schedule-title"])
        .build();
    header_section.append(&schedule_title);

    let analog_clock = analog_clock::build();
    header_section.append(&analog_clock);

    left_column.append(&header_section);

    // All Meetings label
    let meetings_label = gtk::Label::builder()
        .label("All Meetings")
        .xalign(0.0)
        .css_classes(["section-label"])
        .build();
    left_column.append(&meetings_label);

    // Meeting cards container in scrolled window
    let meeting_cards_container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    meeting_cards_container.add_css_class("meeting-cards-container");

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&meeting_cards_container)
        .build();
    scrolled.set_min_content_height(200);
    scrolled.set_propagate_natural_height(true);
    scrolled.add_css_class("meeting-scroller");

    left_column.append(&scrolled);

    // ===== LEFT SIDEBAR =====
    let left_sidebar = gtk::Box::new(gtk::Orientation::Vertical, 12);
    left_sidebar.add_css_class("left-sidebar");
    left_sidebar.set_width_request(220);

    // Quick Actions label
    let actions_label = gtk::Label::builder()
        .label("Quick Actions")
        .xalign(0.0)
        .css_classes(["section-label"])
        .build();
    left_sidebar.append(&actions_label);

    // Action buttons - stacked vertically
    let create_content = adw::ButtonContent::builder()
        .icon_name("video-joined-symbolic")
        .label(LABEL_CREATE_MEET)
        .build();
    let create_button = gtk::Button::builder()
        .child(&create_content)
        .tooltip_text("Create a new Google Meet video call")
        .css_classes(["sidebar-action"])
        .build();

    let snooze_content = adw::ButtonContent::builder()
        .icon_name("alarm-symbolic")
        .label(LABEL_SNOOZE)
        .build();
    let snooze_button = gtk::Button::builder()
        .child(&snooze_content)
        .tooltip_text("Hide notifications for 10 minutes")
        .css_classes(["sidebar-action"])
        .build();

    let calendar_content = adw::ButtonContent::builder()
        .icon_name("x-office-calendar-symbolic")
        .label(LABEL_CALENDAR)
        .build();
    let calendar_button = gtk::Button::builder()
        .child(&calendar_content)
        .tooltip_text("Open today's calendar")
        .css_classes(["sidebar-action"])
        .build();

    let refresh_content = adw::ButtonContent::builder()
        .icon_name("view-refresh-symbolic")
        .label(LABEL_REFRESH)
        .build();
    let refresh_button = gtk::Button::builder()
        .child(&refresh_content)
        .tooltip_text("Refresh meeting list")
        .css_classes(["sidebar-action"])
        .build();

    let clear_content = adw::ButtonContent::builder()
        .icon_name("edit-clear-all-symbolic")
        .label(LABEL_CLEAR_DISMISSALS)
        .build();
    let clear_dismissals_button = gtk::Button::builder()
        .child(&clear_content)
        .tooltip_text("Show previously dismissed meetings")
        .css_classes(["sidebar-action", "sidebar-action-secondary"])
        .build();

    left_sidebar.append(&create_button);
    left_sidebar.append(&snooze_button);
    left_sidebar.append(&calendar_button);
    left_sidebar.append(&refresh_button);
    left_sidebar.append(&clear_dismissals_button);

    // ===== MAIN CONTAINER =====
    let main_container = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    main_container.add_css_class("main-container");
    main_container.set_margin_top(20);
    main_container.set_margin_bottom(20);
    main_container.set_margin_start(20);
    main_container.set_margin_end(20);
    main_container.append(&left_sidebar);
    main_container.append(&left_column);

    let clamp = adw::Clamp::builder()
        .maximum_size(900)
        .tightening_threshold(600)
        .child(&main_container)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&clamp));

    window.set_content(Some(&toolbar));

    UiWidgets {
        window,
        meeting_cards_container,
        create_button,
        refresh_button,
        snooze_button,
        calendar_button,
        clear_dismissals_button,
        sidebar_toggle_button,
        left_sidebar,
    }
}
