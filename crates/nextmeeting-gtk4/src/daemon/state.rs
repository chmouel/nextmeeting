use nextmeeting_core::MeetingView;

#[derive(Debug, Default)]
pub struct MeetingState {
    meetings: Vec<MeetingView>,
    connected: bool,
}

impl MeetingState {
    pub fn meetings(&self) -> &[MeetingView] {
        &self.meetings
    }

    pub fn set_meetings(&mut self, meetings: Vec<MeetingView>) {
        self.connected = true;
        self.meetings = meetings;
    }

    pub fn set_disconnected(&mut self) {
        self.connected = false;
        self.meetings.clear();
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn remove_meeting(&mut self, event_id: &str) {
        self.meetings.retain(|meeting| meeting.id != event_id);
    }
}
