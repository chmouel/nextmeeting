pub mod manager;
pub mod sni;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ToggleWindow,
    Refresh,
    Quit,
}
