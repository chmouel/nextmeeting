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
    pub dismiss_button: gtk::Button,
    pub decline_button: gtk::Button,
    pub delete_button: gtk::Button,
}

impl MeetingCard {
    /// Creates a new meeting card for the given meeting.
    ///
    /// # Arguments
    /// * `meeting` - The meeting to display
    /// * `show_join_button` - Whether to show the JOIN NOW button (for current/ongoing meetings)
    /// * `always_show_actions` - Whether to always show action buttons (vs only on hover)
    /// * `is_dismissed` - Whether the meeting is dismissed (shown with muted styling when viewing dismissed)
    pub fn new(
        meeting: &MeetingView,
        show_join_button: bool,
        always_show_actions: bool,
        is_dismissed: bool,
    ) -> Self {
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
        if is_dismissed {
            frame.add_css_class("meeting-card-dismissed");
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

        // Title - show full title as tooltip when truncated
        let title_label = gtk::Label::builder()
            .label(truncate(&meeting.title, 60))
            .tooltip_text(&meeting.title)
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

        // Right side: action buttons container (shown on hover, or always for first card)
        let action_buttons_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        action_buttons_box.set_valign(gtk::Align::Center);
        action_buttons_box.add_css_class("meeting-card-actions");
        if always_show_actions {
            action_buttons_box.add_css_class("meeting-card-actions-visible");
        }

        // Join button (optional)
        let join_button = if show_join_button && meeting.primary_link.is_some() {
            let btn_content = adw::ButtonContent::builder()
                .icon_name("call-start-symbolic")
                .label("JOIN NOW")
                .build();
            let btn = gtk::Button::builder()
                .child(&btn_content)
                .tooltip_text("Open video meeting link")
                .css_classes(["suggested-action", "meeting-card-join"])
                .valign(gtk::Align::Center)
                .build();
            hbox.append(&btn);
            Some(btn)
        } else {
            None
        };

        // Action buttons (dismiss, decline, delete) - shown on hover
        let (dismiss_icon, dismiss_tooltip) = if is_dismissed {
            ("view-restore-symbolic", "Restore this event")
        } else {
            ("window-close-symbolic", "Dismiss this event (hide locally)")
        };
        let dismiss_button = gtk::Button::builder()
            .icon_name(dismiss_icon)
            .tooltip_text(dismiss_tooltip)
            .css_classes(["flat", "circular", "meeting-card-action"])
            .valign(gtk::Align::Center)
            .build();

        let decline_button = gtk::Button::builder()
            .icon_name("call-stop-symbolic")
            .tooltip_text("Decline this event")
            .css_classes(["flat", "circular", "meeting-card-action"])
            .valign(gtk::Align::Center)
            .build();

        let delete_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Delete this event")
            .css_classes([
                "flat",
                "circular",
                "meeting-card-action",
                "destructive-action",
            ])
            .valign(gtk::Align::Center)
            .build();

        action_buttons_box.append(&delete_button);
        action_buttons_box.append(&dismiss_button);
        action_buttons_box.append(&decline_button);

        hbox.append(&action_buttons_box);

        frame.set_child(Some(&hbox));

        Self {
            widget: frame,
            join_button,
            dismiss_button,
            decline_button,
            delete_button,
        }
    }

    /// Returns the GTK widget for this card.
    pub fn widget(&self) -> &gtk::Frame {
        &self.widget
    }
}
