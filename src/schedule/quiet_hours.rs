use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Tz;
use color_eyre::eyre::{Context, Result};

use crate::config::Settings;

/// 計画時刻が静かな時間帯（要件: 0:00〜8:00 JST 等）に入る場合、当日の `quiet_hours_end` にずらす。
pub fn adjust_for_quiet_hours(scheduled_at: DateTime<Utc>, settings: &Settings) -> Result<DateTime<Utc>> {
    adjust_for_quiet_hours_at(scheduled_at, settings)
}

pub fn adjust_for_quiet_hours_at(
    scheduled_at: DateTime<Utc>,
    settings: &Settings,
) -> Result<DateTime<Utc>> {
    let timezone: Tz = settings
        .timezone
        .parse()
        .wrap_err_with(|| format!("invalid timezone: {}", settings.timezone))?;

    let local = scheduled_at.with_timezone(&timezone);
    let hour = local.hour();

    if hour >= settings.quiet_hours_start && hour < settings.quiet_hours_end {
        let adjusted = local
            .date_naive()
            .and_hms_opt(settings.quiet_hours_end, 0, 0)
            .ok_or_else(|| color_eyre::eyre::eyre!("invalid quiet_hours_end"))?
            .and_local_timezone(timezone)
            .single()
            .ok_or_else(|| color_eyre::eyre::eyre!("ambiguous quiet-hours boundary"))?;

        Ok(adjusted.with_timezone(&Utc))
    } else {
        Ok(scheduled_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
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
    fn shifts_early_morning_to_eight() {
        let settings = settings();
        // 2026-05-21 03:00 JST
        let scheduled = Tokyo.with_ymd_and_hms(2026, 5, 21, 3, 0, 0).unwrap();

        let adjusted = adjust_for_quiet_hours_at(scheduled.with_timezone(&Utc), &settings).unwrap();
        let local = adjusted.with_timezone(&Tokyo);

        assert_eq!(local.hour(), 8);
        assert_eq!(local.minute(), 0);
        assert_eq!(local.date_naive(), scheduled.date_naive());
    }

    #[test]
    fn leaves_daytime_unchanged() {
        let settings = settings();
        let scheduled = Tokyo.with_ymd_and_hms(2026, 5, 21, 10, 30, 0).unwrap();

        let adjusted = adjust_for_quiet_hours_at(scheduled.with_timezone(&Utc), &settings).unwrap();

        assert_eq!(adjusted, scheduled.with_timezone(&Utc));
    }

    #[test]
    fn eight_oclock_is_not_quiet() {
        let settings = settings();
        let scheduled = Tokyo.with_ymd_and_hms(2026, 5, 21, 8, 0, 0).unwrap();

        let adjusted = adjust_for_quiet_hours_at(scheduled.with_timezone(&Utc), &settings).unwrap();

        assert_eq!(adjusted, scheduled.with_timezone(&Utc));
    }

    #[test]
    fn seven_fifty_nine_is_quiet() {
        let settings = settings();
        let scheduled = Tokyo.with_ymd_and_hms(2026, 5, 21, 7, 59, 0).unwrap();

        let adjusted = adjust_for_quiet_hours_at(scheduled.with_timezone(&Utc), &settings).unwrap();
        let local = adjusted.with_timezone(&Tokyo);

        assert_eq!(local.hour(), 8);
        assert_eq!(local.minute(), 0);
    }
}
