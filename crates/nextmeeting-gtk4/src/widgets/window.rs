use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

#[derive(Clone)]
pub struct UiWidgets {
    pub window: adw::ApplicationWindow,
    pub toast_overlay: adw::ToastOverlay,
    pub meeting_cards_container: gtk::Box,
    pub create_button: gtk::Button,
    pub refresh_button: gtk::Button,
    pub snooze_button: gtk::Button,
    pub calendar_button: gtk::Button,
    pub clear_dismissals_button: gtk::Button,
}

pub fn build(app: &adw::Application, snooze_minutes: u32) -> UiWidgets {
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

    // ===== LEFT COLUMN =====
    let left_column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    left_column.add_css_class("left-column");
    left_column.set_hexpand(true);

    // Header section with title
    let header_section = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    header_section.set_valign(gtk::Align::Start);

    let schedule_title = gtk::Label::builder()
        .label("Today's Schedule")
        .xalign(0.0)
        .css_classes(["schedule-title"])
        .build();

    let date_label = gtk::Label::builder()
        .label(chrono::Local::now().format("%A, %B %-d").to_string())
        .xalign(0.0)
        .css_classes(["schedule-date"])
        .build();

    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    title_box.set_hexpand(true);
    title_box.append(&schedule_title);
    title_box.append(&date_label);
    header_section.append(&title_box);

    left_column.append(&header_section);

    // All Meetings label
    let meetings_label = gtk::Label::builder()
        .label("All Meetings")
        .xalign(0.0)
        .css_classes(["section-label"])
        .build();
    left_column.append(&meetings_label);

    // Meeting cards container in scrolled window
    let meeting_cards_container = gtk::Box::new(gtk::Orientation::Vertical, 4);
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

    // ===== LEFT SIDEBAR (Icons only) =====
    let left_sidebar = gtk::Box::new(gtk::Orientation::Vertical, 12);
    left_sidebar.add_css_class("left-sidebar");
    left_sidebar.set_width_request(64);

    // Action buttons - icons only with tooltips
    let create_button = gtk::Button::builder()
        .icon_name("camera-video-symbolic")
        .tooltip_text("Create a new Google Meet video call")
        .css_classes(["sidebar-action"])
        .build();

    let snooze_button = gtk::Button::builder()
        .icon_name("alarm-symbolic")
        .tooltip_text(format!(
            "Hide notifications for {} minutes",
            snooze_minutes
        ))
        .css_classes(["sidebar-action"])
        .build();

    let calendar_button = gtk::Button::builder()
        .icon_name("calendar-month-symbolic")
        .tooltip_text("Open today's calendar")
        .css_classes(["sidebar-action"])
        .build();

    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh meeting list")
        .css_classes(["sidebar-action"])
        .build();

    let clear_dismissals_button = gtk::Button::builder()
        .icon_name("edit-clear-symbolic")
        .tooltip_text("Show previously dismissed meetings")
        .css_classes(["sidebar-action", "sidebar-action-secondary"])
        .build();

    left_sidebar.append(&create_button);
    left_sidebar.append(&snooze_button);
    left_sidebar.append(&calendar_button);
    left_sidebar.append(&refresh_button);
    left_sidebar.append(&clear_dismissals_button);

    // ===== MAIN CONTAINER =====
    let main_container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    main_container.add_css_class("main-container");
    main_container.set_margin_top(12);
    main_container.set_margin_bottom(12);
    main_container.set_margin_start(12);
    main_container.set_margin_end(12);
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

    // Wrap in toast overlay for notifications
    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&toolbar));

    window.set_content(Some(&toast_overlay));

    UiWidgets {
        window,
        toast_overlay,
        meeting_cards_container,
        create_button,
        refresh_button,
        snooze_button,
        calendar_button,
        clear_dismissals_button,
    }
}
