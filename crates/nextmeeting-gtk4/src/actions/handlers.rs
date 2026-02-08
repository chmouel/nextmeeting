use crate::application::AppRuntime;

pub fn join(runtime: &AppRuntime) -> Result<(), String> {
    runtime.open_next_meeting()
}

pub fn create(runtime: &AppRuntime, service: &str) -> Result<(), String> {
    runtime.create_meeting(service, None)
}

pub async fn refresh(runtime: &AppRuntime) -> Result<(), String> {
    runtime.force_refresh().await
}

pub async fn snooze(runtime: &AppRuntime, minutes: u32) -> Result<(), String> {
    runtime.snooze(minutes).await
}

pub fn open_calendar_day(runtime: &AppRuntime) -> Result<(), String> {
    runtime.open_calendar_day()
}
