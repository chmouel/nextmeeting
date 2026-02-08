use std::sync::mpsc::Sender;

use ksni::{MenuItem, menu};

use super::TrayCommand;

#[derive(Debug)]
pub struct NextMeetingTray {
    tx: Sender<TrayCommand>,
}

impl NextMeetingTray {
    pub fn new(tx: Sender<TrayCommand>) -> Self {
        Self { tx }
    }
}

impl ksni::Tray for NextMeetingTray {
    fn id(&self) -> String {
        "nextmeeting-gtk".to_string()
    }

    fn icon_name(&self) -> String {
        "x-office-calendar".to_string()
    }

    fn title(&self) -> String {
        "NextMeeting".to_string()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCommand::ToggleWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            menu::StandardItem {
                label: "Show / Hide".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayCommand::ToggleWindow);
                }),
                ..Default::default()
            }
            .into(),
            menu::StandardItem {
                label: "Refresh".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayCommand::Refresh);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            menu::StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}
