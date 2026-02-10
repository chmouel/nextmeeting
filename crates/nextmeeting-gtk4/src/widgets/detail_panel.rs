//! Right-side detail panel for meeting details.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;
use nextmeeting_core::{MeetingView, ResponseStatus};

use crate::utils::format_time_range;
use crate::widgets::meeting_card::{linkify_for_pango, normalise_description};

/// A slide-in panel that shows full meeting details.
#[derive(Clone)]
pub struct DetailPanel {
    pub revealer: gtk::Revealer,
    close_button: gtk::Button,
    title_label: gtk::Label,
    time_label: gtk::Label,
    location_row: gtk::Box,
    location_label: gtk::Label,
    video_row: gtk::Box,
    video_button: gtk::LinkButton,
    description_label: gtk::Label,
    attendees_header: gtk::Label,
    attendees_list: gtk::Box,
    selected_meeting_id: Rc<RefCell<Option<String>>>,
}

impl DetailPanel {
    pub fn new() -> Self {
        let selected_meeting_id: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        // Outer container with fixed width
        let panel_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel_box.add_css_class("detail-panel");
        panel_box.set_width_request(350);

        // === Header ===
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("detail-panel-header");
        header.set_margin_top(12);
        header.set_margin_bottom(8);
        header.set_margin_start(16);
        header.set_margin_end(8);

        let title_label = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .css_classes(["detail-panel-title"])
            .build();
        header.append(&title_label);

        let close_button = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "detail-panel-close"])
            .valign(gtk::Align::Start)
            .build();
        header.append(&close_button);

        panel_box.append(&header);

        // Separator
        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.set_margin_start(16);
        sep.set_margin_end(16);
        panel_box.append(&sep);

        // === Scrolled content ===
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.add_css_class("detail-panel-content");
        content.set_margin_top(12);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);

        // -- Time section --
        let time_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        time_row.add_css_class("detail-section");
        let time_icon = gtk::Image::from_icon_name("alarm-symbolic");
        time_icon.set_pixel_size(16);
        time_icon.add_css_class("detail-section-icon");
        let time_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .build();
        time_row.append(&time_icon);
        time_row.append(&time_label);
        content.append(&time_row);

        // -- Location section --
        let location_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        location_row.add_css_class("detail-section");
        let loc_icon = gtk::Image::from_icon_name("mark-location-symbolic");
        loc_icon.set_pixel_size(16);
        loc_icon.add_css_class("detail-section-icon");
        let location_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .build();
        location_row.append(&loc_icon);
        location_row.append(&location_label);
        content.append(&location_row);

        // -- Video link section --
        let video_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        video_row.add_css_class("detail-section");
        let vid_icon = gtk::Image::from_icon_name("camera-video-symbolic");
        vid_icon.set_pixel_size(16);
        vid_icon.add_css_class("detail-section-icon");
        let video_button = gtk::LinkButton::builder().label("Join video call").build();
        video_row.append(&vid_icon);
        video_row.append(&video_button);
        content.append(&video_row);

        // -- Description section --
        let desc_header = gtk::Label::builder()
            .label("Description")
            .xalign(0.0)
            .css_classes(["detail-section-header"])
            .build();
        content.append(&desc_header);

        let description_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .selectable(true)
            .css_classes(["detail-description-text"])
            .build();
        content.append(&description_label);

        // -- Attendees section --
        let attendees_header = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["detail-section-header"])
            .build();
        content.append(&attendees_header);

        let attendees_list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.append(&attendees_list);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();

        panel_box.append(&scrolled);

        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(200)
            .reveal_child(false)
            .child(&panel_box)
            .build();

        Self {
            revealer,
            close_button,
            title_label,
            time_label,
            location_row,
            location_label,
            video_row,
            video_button,
            description_label,
            attendees_header,
            attendees_list,
            selected_meeting_id,
        }
    }

    /// Shows the panel with the given meeting's details.
    pub fn show_meeting(&self, meeting: &MeetingView) {
        // Title
        self.title_label.set_label(&meeting.title);

        // Time
        let date_str = meeting.start_local.format("%A, %B %-d").to_string();
        let time_str = format_time_range(meeting.start_local, meeting.end_local);
        self.time_label
            .set_label(&format!("{} \u{2022} {}", time_str, date_str));

        // Location
        match &meeting.location {
            Some(loc) if !loc.is_empty() => {
                self.location_label.set_label(loc);
                self.location_row.set_visible(true);
            }
            _ => {
                self.location_row.set_visible(false);
            }
        }

        // Video link
        match &meeting.primary_link {
            Some(link) if link.kind.is_video_conference() => {
                self.video_button.set_uri(&link.url);
                self.video_button
                    .set_label(&format!("Join {}", link.kind.display_name()));
                self.video_row.set_visible(true);
            }
            _ => {
                self.video_row.set_visible(false);
            }
        }

        // Description
        let desc_text = normalise_description(meeting.description.as_deref())
            .unwrap_or_else(|| "No description provided.".to_string());
        self.description_label
            .set_markup(&linkify_for_pango(&desc_text));

        // Attendees
        // Clear existing
        while let Some(child) = self.attendees_list.first_child() {
            self.attendees_list.remove(&child);
        }

        let count = meeting.attendees.len();
        if count > 0 {
            self.attendees_header
                .set_label(&format!("Attendees ({})", count));
            self.attendees_header.set_visible(true);

            for attendee in &meeting.attendees {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                row.add_css_class("attendee-row");

                // Status icon
                let (icon_name, css_class) = match attendee.response_status {
                    ResponseStatus::Accepted => ("emblem-ok-symbolic", "attendee-accepted"),
                    ResponseStatus::Declined => ("window-close-symbolic", "attendee-declined"),
                    ResponseStatus::Tentative => ("dialog-question-symbolic", "attendee-tentative"),
                    ResponseStatus::NeedsAction | ResponseStatus::Unknown => {
                        ("mail-unread-symbolic", "attendee-pending")
                    }
                };
                let icon = gtk::Image::from_icon_name(icon_name);
                icon.set_pixel_size(14);
                icon.add_css_class(css_class);
                row.append(&icon);

                // Name
                let name_label = gtk::Label::builder()
                    .label(&attendee.display_name)
                    .xalign(0.0)
                    .hexpand(true)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .css_classes(["attendee-name"])
                    .tooltip_text(&attendee.email)
                    .build();
                row.append(&name_label);

                // Badges
                if attendee.organizer {
                    let badge = gtk::Label::builder()
                        .label("Organizer")
                        .css_classes(["attendee-badge"])
                        .build();
                    row.append(&badge);
                }
                if attendee.optional {
                    let badge = gtk::Label::builder()
                        .label("Optional")
                        .css_classes(["attendee-badge"])
                        .build();
                    row.append(&badge);
                }

                self.attendees_list.append(&row);
            }
        } else {
            self.attendees_header.set_visible(false);
        }

        // Store ID and reveal
        *self.selected_meeting_id.borrow_mut() = Some(meeting.id.clone());
        self.revealer.set_reveal_child(true);
    }

    /// Hides the panel and clears the selected meeting.
    pub fn hide(&self) {
        self.revealer.set_reveal_child(false);
        *self.selected_meeting_id.borrow_mut() = None;
    }

    /// Returns the currently displayed meeting ID, if any.
    pub fn current_meeting_id(&self) -> Option<String> {
        self.selected_meeting_id.borrow().clone()
    }

    /// Returns a reference to the close button.
    pub fn close_button(&self) -> &gtk::Button {
        &self.close_button
    }
}
