//! Meeting card widget for the NextMeeting GTK4 UI.

use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

use chrono::Local;
use nextmeeting_core::MeetingView;

use crate::utils::{format_time_range, truncate};

/// A meeting card widget displaying meeting info with color coding.
pub struct MeetingCard {
    pub widget: gtk::Frame,
    pub join_button: Option<gtk::Button>,
    pub calendar_button: gtk::Button,
    pub dismiss_button: gtk::Button,
    pub decline_button: gtk::Button,
    pub delete_button: gtk::Button,
    pub description_revealer: gtk::Revealer,
    pub description_label: gtk::Label,
}

pub fn normalise_description(description: Option<&str>) -> Option<String> {
    description
        .map(html_to_text)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn html_to_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            let mut tag = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == '>' {
                    closed = true;
                    break;
                }
                tag.push(next);
            }

            if !closed {
                output.push('<');
                output.push_str(&tag);
                continue;
            }

            let tag = tag.trim().to_ascii_lowercase();
            if matches!(tag.as_str(), "br" | "br/" | "p" | "/p" | "div" | "/div")
                && !output.ends_with('\n')
            {
                output.push('\n');
            } else if tag.starts_with("li") {
                if !output.is_empty() && !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str("• ");
            } else if tag == "/li" && !output.ends_with('\n') {
                output.push('\n');
            }
            continue;
        }

        if ch == '&' {
            let mut entity = String::new();
            let mut closed = false;
            while let Some(next) = chars.peek().copied() {
                entity.push(next);
                chars.next();
                if next == ';' {
                    closed = true;
                    break;
                }
                if entity.len() > 12 {
                    break;
                }
            }

            if closed {
                match decode_html_entity(&entity) {
                    Some(decoded) => output.push_str(&decoded),
                    None => {
                        output.push('&');
                        output.push_str(&entity);
                    }
                }
                continue;
            }

            output.push('&');
            output.push_str(&entity);
            continue;
        }

        output.push(ch);
    }

    collapse_text_whitespace(&output)
}

fn decode_html_entity(entity_with_semicolon: &str) -> Option<String> {
    let entity = entity_with_semicolon.strip_suffix(';')?;
    let decoded = match entity {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        "nbsp" => " ".to_string(),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            let value = u32::from_str_radix(&entity[2..], 16).ok()?;
            char::from_u32(value)?.to_string()
        }
        _ if entity.starts_with('#') => {
            let value = entity[1..].parse::<u32>().ok()?;
            char::from_u32(value)?.to_string()
        }
        _ => return None,
    };
    Some(decoded)
}

fn collapse_text_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut saw_space = false;
    let mut newline_run = 0usize;

    for ch in input.chars() {
        if ch == '\n' {
            if output.ends_with(' ') {
                output.pop();
            }
            if newline_run < 2 {
                output.push('\n');
            }
            newline_run += 1;
            saw_space = false;
            continue;
        }

        newline_run = 0;
        if ch.is_whitespace() {
            if !saw_space && !output.ends_with('\n') {
                output.push(' ');
                saw_space = true;
            }
        } else {
            output.push(ch);
            saw_space = false;
        }
    }

    output.trim().to_string()
}

impl MeetingCard {
    /// Creates a new meeting card for the given meeting.
    ///
    /// # Arguments
    /// * `meeting` - The meeting to display
    /// * `show_join_button` - Whether to show the JOIN NOW button (for current/ongoing meetings)
    /// * `always_show_actions` - Whether to always show action buttons (vs only on hover)
    /// * `is_dismissed` - Whether the meeting is dismissed (shown with muted styling when viewing dismissed)
    /// * `is_soon` - Whether the meeting starts within 5 minutes (shown with warning styling)
    pub fn new(
        meeting: &MeetingView,
        show_join_button: bool,
        is_primary: bool,
        always_show_actions: bool,
        is_dismissed: bool,
        is_soon: bool,
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
        let now = Local::now();
        let is_ongoing = meeting.start_local <= now && now < meeting.end_local;
        if is_ongoing {
            frame.add_css_class("meeting-card-ongoing");
        } else if is_soon && !is_dismissed {
            frame.add_css_class("meeting-card-soon");
        }
        if is_dismissed {
            frame.add_css_class("meeting-card-dismissed");
        }

        let root_box = gtk::Box::new(gtk::Orientation::Vertical, 8);

        // Main horizontal box
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hbox.set_margin_top(12);
        hbox.set_margin_bottom(4);
        hbox.set_margin_start(14);
        hbox.set_margin_end(14);

        // Icon
        let icon_name = if is_video {
            "camera-video-symbolic"
        } else {
            "calendar-month-symbolic"
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
        let time_text = if is_ongoing {
            let minutes_left = meeting.minutes_until_end(now);
            let time_remaining = if minutes_left <= 0 {
                "Ending".to_string()
            } else if minutes_left < 60 {
                format!("{} minutes to go", minutes_left)
            } else {
                let hours = minutes_left / 60;
                let mins = minutes_left % 60;
                if mins == 0 {
                    format!("{}H to go", hours)
                } else {
                    format!("{}H{:02} to go", hours, mins)
                }
            };
            format!("Now • {}", time_remaining)
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
        if is_ongoing {
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
                .css_classes([
                    "suggested-action",
                    "meeting-card-join",
                    "meeting-card-interactive-action",
                ])
                .valign(gtk::Align::Center)
                .build();

            // Add conditional visibility class
            if is_primary {
                btn.add_css_class("meeting-card-join-primary");
            } else {
                btn.add_css_class("meeting-card-join-secondary");
            }

            if is_soon {
                btn.add_css_class("meeting-card-join-soon");
            }

            // Add to action_buttons_box (leftmost position)
            action_buttons_box.prepend(&btn);
            Some(btn)
        } else {
            None
        };

        // Action buttons (dismiss, decline, delete) - shown on hover
        let (dismiss_icon, dismiss_tooltip) = if is_dismissed {
            ("edit-undo-symbolic", "Restore this event")
        } else {
            ("window-close-symbolic", "Dismiss this event (hide locally)")
        };
        let dismiss_button = gtk::Button::builder()
            .icon_name(dismiss_icon)
            .tooltip_text(dismiss_tooltip)
            .css_classes([
                "flat",
                "circular",
                "meeting-card-action",
                "meeting-card-interactive-action",
            ])
            .valign(gtk::Align::Center)
            .build();

        let decline_button = gtk::Button::builder()
            .icon_name("call-stop-symbolic")
            .tooltip_text("Decline this event")
            .css_classes([
                "flat",
                "circular",
                "meeting-card-action",
                "meeting-card-interactive-action",
            ])
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
                "meeting-card-interactive-action",
            ])
            .valign(gtk::Align::Center)
            .build();

        let calendar_button = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text(if meeting.calendar_url.is_some() {
                "Edit this calendar event"
            } else {
                "No calendar event URL available"
            })
            .css_classes([
                "flat",
                "circular",
                "meeting-card-action",
                "meeting-card-interactive-action",
            ])
            .valign(gtk::Align::Center)
            .sensitive(meeting.calendar_url.is_some())
            .build();

        action_buttons_box.append(&delete_button);
        action_buttons_box.append(&dismiss_button);
        action_buttons_box.append(&decline_button);
        action_buttons_box.append(&calendar_button);

        hbox.append(&action_buttons_box);
        root_box.append(&hbox);

        let description_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .selectable(true)
            .css_classes(["meeting-description-inline-body"])
            .build();

        let description_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(72)
            .max_content_height(220)
            .child(&description_label)
            .build();
        description_scroll.add_css_class("meeting-description-inline-scroll");

        let description_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        description_box.add_css_class("meeting-description-inline");
        description_box.set_margin_top(0);
        description_box.set_margin_bottom(10);
        description_box.set_margin_start(14);
        description_box.set_margin_end(14);
        description_box.append(&description_scroll);

        let description_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(170)
            .reveal_child(false)
            .child(&description_box)
            .build();
        root_box.append(&description_revealer);
        frame.set_child(Some(&root_box));

        Self {
            widget: frame,
            join_button,
            calendar_button,
            dismiss_button,
            decline_button,
            delete_button,
            description_revealer,
            description_label,
        }
    }

    pub fn set_description_text(&self, description: Option<&str>) {
        let text = normalise_description(description)
            .unwrap_or_else(|| "No description provided for this event.".to_string());
        self.description_label.set_label(&text);
    }

    /// Returns the GTK widget for this card.
    pub fn widget(&self) -> &gtk::Frame {
        &self.widget
    }
}

#[cfg(test)]
mod tests {
    use super::normalise_description;

    #[test]
    fn normalise_description_preserves_text() {
        assert_eq!(
            normalise_description(Some("Weekly sync")),
            Some("Weekly sync".to_string())
        );
    }

    #[test]
    fn normalise_description_trims_text() {
        assert_eq!(
            normalise_description(Some("  Weekly sync  ")),
            Some("Weekly sync".to_string())
        );
    }

    #[test]
    fn normalise_description_empty_becomes_none() {
        assert_eq!(normalise_description(Some("")), None);
    }

    #[test]
    fn normalise_description_whitespace_becomes_none() {
        assert_eq!(normalise_description(Some("   ")), None);
    }

    #[test]
    fn normalise_description_none_stays_none() {
        assert_eq!(normalise_description(None), None);
    }

    #[test]
    fn normalise_description_strips_html_tags() {
        assert_eq!(
            normalise_description(Some("<p>Hello<br>world</p>")),
            Some("Hello\nworld".to_string())
        );
    }

    #[test]
    fn normalise_description_decodes_html_entities() {
        assert_eq!(
            normalise_description(Some("Tom &amp; Jerry &lt;3")),
            Some("Tom & Jerry <3".to_string())
        );
    }

    #[test]
    fn normalise_description_handles_list_items() {
        assert_eq!(
            normalise_description(Some("<ul><li>One</li><li>Two</li></ul>")),
            Some("• One\n• Two".to_string())
        );
    }

    #[test]
    fn normalise_description_html_only_becomes_none() {
        assert_eq!(normalise_description(Some("<p>   </p>")), None);
    }
}
