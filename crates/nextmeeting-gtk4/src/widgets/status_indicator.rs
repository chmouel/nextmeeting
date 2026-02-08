#[derive(Debug, Default)]
pub struct StatusIndicator {
    connected: bool,
}

impl StatusIndicator {
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    pub fn connected(&self) -> bool {
        self.connected
    }
}
