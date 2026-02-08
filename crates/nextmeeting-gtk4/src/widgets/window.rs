use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

#[derive(Clone)]
pub struct UiWidgets {
    pub window: adw::ApplicationWindow,
    pub status_label: gtk::Label,
    pub hero_kicker_label: gtk::Label,
    pub hero_title_label: gtk::Label,
    pub hero_meta_label: gtk::Label,
    pub hero_service_label: gtk::Label,
    pub listbox: gtk::ListBox,
    pub join_button: gtk::Button,
    pub create_button: gtk::Button,
    pub refresh_button: gtk::Button,
    pub snooze_button: gtk::Button,
    pub calendar_button: gtk::Button,
    pub clear_dismissals_button: gtk::Button,
}

pub fn build(app: &adw::Application) -> UiWidgets {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("NextMeeting")
        .default_width(480)
        .default_height(640)
        .build();
    window.add_css_class("nm-window");

    let header = adw::HeaderBar::new();
    let window_title = adw::WindowTitle::builder()
        .title("NextMeeting")
        .subtitle("Today")
        .build();
    header.set_title_widget(Some(&window_title));

    let status_label = gtk::Label::builder()
        .label("Waiting for daemon…")
        .xalign(0.0)
        .css_classes(["status-pill"])
        .build();

    let hero_kicker_label = gtk::Label::builder()
        .label("Up next")
        .xalign(0.0)
        .css_classes(["hero-kicker"])
        .build();

    let hero_title_label = gtk::Label::builder()
        .label("No upcoming meetings")
        .xalign(0.0)
        .wrap(true)
        .css_classes(["hero-title"])
        .build();

    let hero_meta_label = gtk::Label::builder()
        .label("Refresh to fetch meetings")
        .xalign(0.0)
        .css_classes(["hero-meta"])
        .build();

    let hero_service_label = gtk::Label::builder()
        .label("No link")
        .xalign(0.0)
        .css_classes(["service-badge"])
        .build();

    // Join button with icon
    let join_content = adw::ButtonContent::builder()
        .icon_name("call-start-symbolic")
        .label("Join")
        .build();
    let join_button = gtk::Button::builder()
        .child(&join_content)
        .css_classes(["suggested-action", "pill-button", "join-button"])
        .sensitive(false)
        .build();

    let hero_header = gtk::Box::new(gtk::Orientation::Vertical, 6);
    hero_header.append(&hero_kicker_label);
    hero_header.append(&hero_title_label);
    hero_header.append(&hero_meta_label);

    let hero_footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero_footer.append(&hero_service_label);
    hero_footer.append(&join_button);

    let hero_body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    hero_body.add_css_class("hero-card");
    hero_body.set_margin_top(16);
    hero_body.set_margin_bottom(16);
    hero_body.set_margin_start(16);
    hero_body.set_margin_end(16);
    hero_body.append(&hero_header);
    hero_body.append(&hero_footer);

    let hero_frame = gtk::Frame::new(None);
    hero_frame.set_child(Some(&hero_body));
    hero_frame.add_css_class("hero-frame");

    // Action buttons with icons
    let create_content = adw::ButtonContent::builder()
        .icon_name("video-joined-symbolic")
        .label("Create meet")
        .build();
    let create_button = gtk::Button::builder()
        .child(&create_content)
        .css_classes(["pill-button", "action-button"])
        .build();

    let refresh_content = adw::ButtonContent::builder()
        .icon_name("view-refresh-symbolic")
        .label("Refresh")
        .build();
    let refresh_button = gtk::Button::builder()
        .child(&refresh_content)
        .css_classes(["suggested-action", "pill-button", "action-button"])
        .build();

    let snooze_content = adw::ButtonContent::builder()
        .icon_name("alarm-symbolic")
        .label("Snooze 10m")
        .build();
    let snooze_button = gtk::Button::builder()
        .child(&snooze_content)
        .css_classes(["pill-button", "action-button", "action-button-secondary"])
        .build();

    let calendar_content = adw::ButtonContent::builder()
        .icon_name("x-office-calendar-symbolic")
        .label("Calendar")
        .build();
    let calendar_button = gtk::Button::builder()
        .child(&calendar_content)
        .css_classes(["pill-button", "action-button", "action-button-secondary"])
        .build();

    let clear_content = adw::ButtonContent::builder()
        .icon_name("edit-clear-all-symbolic")
        .label("Clear dismissals")
        .build();
    let clear_dismissals_button = gtk::Button::builder()
        .child(&clear_content)
        .css_classes(["pill-button", "action-button", "action-button-secondary"])
        .build();

    let actions_label = gtk::Label::builder()
        .label("Quick actions")
        .xalign(0.0)
        .css_classes(["section-label"])
        .build();

    let actions_primary = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    actions_primary.append(&create_button);
    actions_primary.append(&refresh_button);

    let actions_secondary = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    actions_secondary.append(&snooze_button);
    actions_secondary.append(&calendar_button);
    actions_secondary.append(&clear_dismissals_button);

    let listbox = gtk::ListBox::new();
    listbox.set_selection_mode(gtk::SelectionMode::None);
    listbox.add_css_class("meeting-list");
    listbox.add_css_class("boxed-list");

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&listbox)
        .build();
    scrolled.set_min_content_height(200);
    scrolled.set_propagate_natural_height(true);
    scrolled.add_css_class("meeting-scroller");

    let agenda_label = gtk::Label::builder()
        .label("Today")
        .xalign(0.0)
        .css_classes(["section-label"])
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
    root.add_css_class("root-shell");
    root.set_margin_top(20);
    root.set_margin_bottom(20);
    root.set_margin_start(20);
    root.set_margin_end(20);
    root.append(&status_label);
    root.append(&hero_frame);
    root.append(&actions_label);
    root.append(&actions_primary);
    root.append(&actions_secondary);
    root.append(&agenda_label);
    root.append(&scrolled);

    let clamp = adw::Clamp::builder()
        .maximum_size(760)
        .tightening_threshold(540)
        .child(&root)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&clamp));

    window.set_content(Some(&toolbar));

    UiWidgets {
        window,
        status_label,
        hero_kicker_label,
        hero_title_label,
        hero_meta_label,
        hero_service_label,
        listbox,
        join_button,
        create_button,
        refresh_button,
        snooze_button,
        calendar_button,
        clear_dismissals_button,
    }
}
