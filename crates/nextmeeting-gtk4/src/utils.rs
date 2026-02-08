use chrono::{DateTime, Local};

pub fn format_time_range(start: DateTime<Local>, end: DateTime<Local>) -> String {
    format!("{} - {}", start.format("%H:%M"), end.format("%H:%M"))
}

pub fn truncate(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len {
        return input.to_string();
    }
    if max_len <= 1 {
        return "…".to_string();
    }
    let truncated: String = input.chars().take(max_len - 1).collect();
    format!("{}…", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_kept() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_shortened() {
        assert_eq!(truncate("hello world", 6), "hello…");
    }
}
