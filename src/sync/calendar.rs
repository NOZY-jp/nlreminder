use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use sqlx::SqlitePool;

use crate::caldav::{CalDavClient, CalDavEvent, default_fetch_range_at};
use crate::config::Settings;
use crate::models::{UpsertAction, mark_in_range_missing_completed, upsert_from_caldav};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncCalendarReport {
    pub upserted: u32,
    pub completed: u32,
}

pub async fn sync_caldav_to_db(
    pool: &SqlitePool,
    client: &CalDavClient,
    settings: &Settings,
) -> Result<SyncCalendarReport> {
    sync_caldav_to_db_at(pool, client, settings, Utc::now()).await
}

pub async fn sync_caldav_to_db_at(
    pool: &SqlitePool,
    client: &CalDavClient,
    settings: &Settings,
    now: DateTime<Utc>,
) -> Result<SyncCalendarReport> {
    let (range_start, range_end) = default_fetch_range_at(now, settings)?;
    let events = client.fetch_events(range_start, range_end).await?;
    sync_fetched_events_to_db(pool, &events, range_start, range_end, now).await
}

pub async fn sync_fetched_events_to_db(
    pool: &SqlitePool,
    events: &[CalDavEvent],
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<SyncCalendarReport> {
    let mut upserted = 0u32;
    let mut seen_uids = Vec::with_capacity(events.len());

    for event in events {
        seen_uids.push(event.uid.clone());
        let action = upsert_from_caldav(
            pool,
            &event.uid,
            &event.summary,
            event.starts_at,
            event.ends_at,
            event.all_day,
            event.etag.as_deref(),
            event.href.as_deref(),
            now,
        )
        .await?;
        if action == UpsertAction::Inserted || action == UpsertAction::Updated {
            upserted += 1;
        }
    }

    let completed =
        mark_in_range_missing_completed(pool, &seen_uids, range_start, range_end, now).await?;

    Ok(SyncCalendarReport {
        upserted,
        completed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;
    use crate::models::{CalendarEventState, find_by_external_uid, upsert_from_caldav};
    use uuid::Uuid;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("nlreminder-sync-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    fn sample_event(uid: &str, title: &str, starts_at: DateTime<Utc>) -> CalDavEvent {
        CalDavEvent {
            uid: uid.to_owned(),
            etag: Some(format!("etag-{uid}")),
            href: Some(format!("/events/{uid}.ics")),
            summary: title.to_owned(),
            starts_at: Some(starts_at),
            ends_at: None,
            all_day: false,
        }
    }

    #[tokio::test]
    async fn sync_fetched_events_upserts_and_completes_missing() {
        let pool = test_pool().await;
        let start = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let end = start + chrono::Duration::days(7);
        let at = start + chrono::Duration::hours(12);
        let event_time = start + chrono::Duration::hours(10);

        upsert_from_caldav(
            &pool,
            "stale",
            "Stale",
            Some(event_time),
            None,
            false,
            None,
            None,
            start,
        )
        .await
        .unwrap();

        let events = vec![sample_event("fresh", "Fresh", event_time)];
        let report = sync_fetched_events_to_db(&pool, &events, start, end, at)
            .await
            .unwrap();

        assert_eq!(report.upserted, 1);
        assert_eq!(report.completed, 1);

        let fresh = find_by_external_uid(&pool, "fresh").await.unwrap().unwrap();
        assert_eq!(fresh.title, "Fresh");
        let stale = find_by_external_uid(&pool, "stale").await.unwrap().unwrap();
        assert_eq!(stale.state, CalendarEventState::Completed);
    }
}
