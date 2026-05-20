use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use color_eyre::eyre::{Context, Result};

use crate::config::Settings;

/// 要件: カレンダー予定を **1 週間分** 取得（当日 0:00 ローカルから 7 日間）。
pub fn default_fetch_range(settings: &Settings) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    default_fetch_range_at(Utc::now(), settings)
}

pub fn default_fetch_range_at(
    now: DateTime<Utc>,
    settings: &Settings,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let timezone: Tz = settings
        .timezone
        .parse()
        .wrap_err_with(|| format!("invalid timezone: {}", settings.timezone))?;
    let local_now = now.with_timezone(&timezone);
    let start = local_now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| color_eyre::eyre::eyre!("invalid local midnight"))?
        .and_local_timezone(timezone)
        .single()
        .ok_or_else(|| color_eyre::eyre::eyre!("ambiguous local midnight"))?;
    let end = start + chrono::Duration::days(7);
    Ok((start.with_timezone(&Utc), end.with_timezone(&Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone as _, Timelike};
    use chrono_tz::Asia::Tokyo;

    fn settings() -> Settings {
        Settings {
            timezone: "Asia/Tokyo".to_owned(),
            morning_summary_hour: 8,
            quiet_hours_start: 0,
            quiet_hours_end: 8,
            scan_interval_secs: 300,
            database_path: "data/nlreminder.db".into(),
            backup_dir: "backups".into(),
            google_tasks_list_id: String::new(),
            caldav_calendar_path: String::new(),
        }
    }

    #[test]
    fn one_week_from_local_midnight() {
        let settings = settings();
        // 2026-05-21 15:30 JST（当日の途中）
        let now = Tokyo.with_ymd_and_hms(2026, 5, 21, 15, 30, 0).unwrap();

        let (start, end) = default_fetch_range_at(now.with_timezone(&Utc), &settings).unwrap();

        let start_local = start.with_timezone(&Tokyo);
        let end_local = end.with_timezone(&Tokyo);

        assert_eq!(start_local.hour(), 0);
        assert_eq!(start_local.minute(), 0);
        assert_eq!(start_local.date_naive().day(), 21);
        assert_eq!((end_local - start_local).num_days(), 7);
    }

    #[test]
    fn invalid_timezone_errors() {
        let mut settings = settings();
        settings.timezone = "Not/AZone".to_owned();

        let err = default_fetch_range_at(Utc::now(), &settings).unwrap_err();

        assert!(err.to_string().contains("invalid timezone"));
    }
}
