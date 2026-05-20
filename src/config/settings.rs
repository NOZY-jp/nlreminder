use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub timezone: String,
    pub morning_summary_hour: u32,
    pub quiet_hours_start: u32,
    pub quiet_hours_end: u32,
    pub scan_interval_secs: u64,
    pub database_path: PathBuf,
    pub backup_dir: PathBuf,
    #[serde(default)]
    pub google_tasks_list_id: String,
    #[serde(default)]
    pub caldav_calendar_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_matches_requirements() {
        let example = include_str!("../../config.toml.example");
        let settings: Settings = toml::from_str(example).expect("example config should parse");

        assert_eq!(settings.timezone, "Asia/Tokyo");
        assert_eq!(settings.morning_summary_hour, 8);
        assert_eq!(settings.quiet_hours_start, 0);
        assert_eq!(settings.quiet_hours_end, 8);
        assert_eq!(settings.scan_interval_secs, 300);
        assert!(settings.google_tasks_list_id.is_empty());
        assert_eq!(settings.caldav_calendar_path, "/nozyjp/sshCalendar/");
    }
}
