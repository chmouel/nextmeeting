//! Meeting card widget for the NextMeeting GTK4 UI.

use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

use nextmeeting_core::MeetingView;

use crate::utils::{format_time_range, truncate};

/// A meeting card widget displaying meeting info with color coding.
pub struct MeetingCard {
    pub widget: gtk::Frame,
    pub join_button: Option<gtk::Button>,
}

impl MeetingCard {
    /// Creates a new meeting card for the given meeting.
    ///
    /// # Arguments
    /// * `meeting` - The meeting to display
    /// * `show_join_button` - Whether to show the JOIN NOW button (for current/ongoing meetings)
    pub fn new(meeting: &MeetingView, show_join_button: bool) -> Self {
        let is_video = meeting
            .primary_link
            .as_ref()
            .is_some_and(|link| link.kind.is_video_conference());

        // Outer frame
        let frame = gtk::Frame::new(None);
        frame.add_css_class("meeting-card");
        if is_video {
            frame.add_css_class("meeting-card-video");
        } else {
            frame.add_css_class("meeting-card-calendar");
        }
        if meeting.is_ongoing {
            frame.add_css_class("meeting-card-ongoing");
        }

        // Main horizontal box
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hbox.set_margin_top(12);
        hbox.set_margin_bottom(12);
        hbox.set_margin_start(14);
        hbox.set_margin_end(14);

        // Icon
        let icon_name = if is_video {
            "video-joined-symbolic"
        } else {
            "x-office-calendar-symbolic"
        };
        let icon = gtk::Image::from_icon_name(icon_name);
        icon.set_pixel_size(24);
        icon.add_css_class("meeting-card-icon");
        if is_video {
            icon.add_css_class("meeting-card-icon-video");
        } else {
            icon.add_css_class("meeting-card-icon-calendar");
        }
        hbox.append(&icon);

        // Center content (title + time/service)
        let center_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        center_box.set_hexpand(true);
        center_box.set_valign(gtk::Align::Center);

        // Title
        let title_label = gtk::Label::builder()
            .label(&truncate(&meeting.title, 60))
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .css_classes(["meeting-card-title"])
            .build();
        center_box.append(&title_label);

        // Time and service line
        let time_text = if meeting.is_ongoing {
            format!(
                "Now • {}",
                format_time_range(meeting.start_local, meeting.end_local)
            )
        } else {
            format_time_range(meeting.start_local, meeting.end_local)
        };

        let service_name = meeting
            .primary_link
            .as_ref()
            .map(|link| link.kind.display_name())
            .unwrap_or("");

        let time_service_text = if service_name.is_empty() {
            time_text
        } else {
            format!("{} • {}", time_text, service_name)
        };

        let time_label = gtk::Label::builder()
            .label(&time_service_text)
            .xalign(0.0)
            .css_classes(["meeting-card-time"])
            .build();
        if meeting.is_ongoing {
            time_label.add_css_class("meeting-card-time-ongoing");
        }
        center_box.append(&time_label);

        hbox.append(&center_box);

        // Join button (optional)
        let join_button = if show_join_button && meeting.primary_link.is_some() {
            let btn_content = adw::ButtonContent::builder()
                .icon_name("call-start-symbolic")
                .label("JOIN NOW")
                .build();
            let btn = gtk::Button::builder()
                .child(&btn_content)
                .css_classes(["suggested-action", "meeting-card-join"])
                .valign(gtk::Align::Center)
                .build();
            hbox.append(&btn);
            Some(btn)
        } else {
            None
        };

        frame.set_child(Some(&hbox));

        Self {
            widget: frame,
            join_button,
        }
    }

    /// Returns the GTK widget for this card.
    pub fn widget(&self) -> &gtk::Frame {
        &self.widget
    }
}
