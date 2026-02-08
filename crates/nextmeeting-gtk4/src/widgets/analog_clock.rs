//! Analog clock widget for the NextMeeting GTK4 UI.

use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::cairo;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

const CLOCK_SIZE: i32 = 120;

/// Creates an analog clock widget that displays the current time.
/// The clock updates every 60 seconds.
pub fn build() -> gtk::DrawingArea {
    let clock = gtk::DrawingArea::builder()
        .width_request(CLOCK_SIZE)
        .height_request(CLOCK_SIZE)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .tooltip_text("Current time")
        .build();
    clock.add_css_class("analog-clock");

    let is_dark = Rc::new(Cell::new(false));

    // Detect dark mode from style manager
    let style_manager = adw::StyleManager::default();
    is_dark.set(style_manager.is_dark());

    let is_dark_for_notify = is_dark.clone();
    let clock_for_notify = clock.clone();
    style_manager.connect_dark_notify(move |sm| {
        is_dark_for_notify.set(sm.is_dark());
        clock_for_notify.queue_draw();
    });

    let is_dark_for_draw = is_dark.clone();
    clock.set_draw_func(move |_area, ctx, width, height| {
        draw_clock(ctx, width, height, is_dark_for_draw.get());
    });

    // Update every 60 seconds
    let clock_for_timer = clock.clone();
    glib::timeout_add_seconds_local(60, move || {
        clock_for_timer.queue_draw();
        glib::ControlFlow::Continue
    });

    clock
}

fn draw_clock(ctx: &cairo::Context, width: i32, height: i32, is_dark: bool) {
    let w = width as f64;
    let h = height as f64;
    let center_x = w / 2.0;
    let center_y = h / 2.0;
    let radius = (w.min(h) / 2.0) - 4.0;

    // Colors based on theme
    let (bg_color, border_color, text_color, hand_color, second_hand_color) = if is_dark {
        (
            (0.15, 0.15, 0.17), // dark background
            (0.35, 0.35, 0.38), // border
            (0.85, 0.85, 0.85), // text
            (0.9, 0.9, 0.9),    // hands
            (0.4, 0.7, 1.0),    // accent for minute hand tip
        )
    } else {
        (
            (0.98, 0.98, 0.98), // light background
            (0.75, 0.75, 0.78), // border
            (0.2, 0.2, 0.22),   // text
            (0.15, 0.15, 0.18), // hands
            (0.2, 0.5, 0.9),    // accent
        )
    };

    // Draw clock face background
    ctx.arc(center_x, center_y, radius, 0.0, 2.0 * PI);
    ctx.set_source_rgb(bg_color.0, bg_color.1, bg_color.2);
    let _ = ctx.fill_preserve();
    ctx.set_source_rgb(border_color.0, border_color.1, border_color.2);
    ctx.set_line_width(2.0);
    let _ = ctx.stroke();

    // Draw hour numbers (1-12)
    ctx.set_source_rgb(text_color.0, text_color.1, text_color.2);
    ctx.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    ctx.set_font_size(11.0);

    for hour in 1..=12 {
        let angle = (hour as f64 - 3.0) * PI / 6.0; // -3 to start at 12 o'clock
        let text_radius = radius - 14.0;
        let x = center_x + text_radius * angle.cos();
        let y = center_y + text_radius * angle.sin();

        let text = hour.to_string();
        let extents = ctx.text_extents(&text).unwrap();
        ctx.move_to(x - extents.width() / 2.0, y + extents.height() / 2.0);
        let _ = ctx.show_text(&text);
    }

    // Draw hour tick marks
    for i in 0..12 {
        let angle = (i as f64) * PI / 6.0 - PI / 2.0;
        let inner_radius = radius - 6.0;
        let outer_radius = radius - 2.0;

        let x1 = center_x + inner_radius * angle.cos();
        let y1 = center_y + inner_radius * angle.sin();
        let x2 = center_x + outer_radius * angle.cos();
        let y2 = center_y + outer_radius * angle.sin();

        ctx.set_source_rgb(border_color.0, border_color.1, border_color.2);
        ctx.set_line_width(2.0);
        ctx.move_to(x1, y1);
        ctx.line_to(x2, y2);
        let _ = ctx.stroke();
    }

    // Get current time
    let now = chrono::Local::now();
    let hour = now.hour() % 12;
    let minute = now.minute();
    let second = now.second();

    // Calculate hand angles
    let hour_angle = (hour as f64 + minute as f64 / 60.0) * PI / 6.0 - PI / 2.0;
    let minute_angle = (minute as f64 + second as f64 / 60.0) * PI / 30.0 - PI / 2.0;

    // Draw hour hand
    let hour_length = radius * 0.5;
    ctx.set_source_rgb(hand_color.0, hand_color.1, hand_color.2);
    ctx.set_line_width(4.0);
    ctx.set_line_cap(cairo::LineCap::Round);
    ctx.move_to(center_x, center_y);
    ctx.line_to(
        center_x + hour_length * hour_angle.cos(),
        center_y + hour_length * hour_angle.sin(),
    );
    let _ = ctx.stroke();

    // Draw minute hand
    let minute_length = radius * 0.75;
    ctx.set_source_rgb(
        second_hand_color.0,
        second_hand_color.1,
        second_hand_color.2,
    );
    ctx.set_line_width(2.5);
    ctx.move_to(center_x, center_y);
    ctx.line_to(
        center_x + minute_length * minute_angle.cos(),
        center_y + minute_length * minute_angle.sin(),
    );
    let _ = ctx.stroke();

    // Draw center dot
    ctx.arc(center_x, center_y, 4.0, 0.0, 2.0 * PI);
    ctx.set_source_rgb(hand_color.0, hand_color.1, hand_color.2);
    let _ = ctx.fill();
}

trait TimeExt {
    fn hour(&self) -> u32;
    fn minute(&self) -> u32;
    fn second(&self) -> u32;
}

impl<T: chrono::Timelike> TimeExt for T {
    fn hour(&self) -> u32 {
        chrono::Timelike::hour(self)
    }
    fn minute(&self) -> u32 {
        chrono::Timelike::minute(self)
    }
    fn second(&self) -> u32 {
        chrono::Timelike::second(self)
    }
}
