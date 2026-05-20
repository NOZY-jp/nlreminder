use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::enums::CalendarEventState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub id: Uuid,
    pub external_uid: Option<String>,
    pub external_etag: Option<String>,
    pub external_href: Option<String>,
    pub title: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub state: CalendarEventState,
    pub remind_enabled: bool,
    pub nlreminder_owned: bool,
    pub source_todo_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertAction {
    Inserted,
    Updated,
}

pub async fn find_by_external_uid(
    pool: &SqlitePool,
    external_uid: &str,
) -> Result<Option<CalendarEvent>> {
    let row = sqlx::query_as::<_, CalendarEventRow>(
        r#"
        SELECT id, external_uid, external_etag, external_href, title,
               starts_at, ends_at, all_day, state, remind_enabled,
               nlreminder_owned, source_todo_id, created_at, updated_at
        FROM calendar_events
        WHERE external_uid = ?
        "#,
    )
    .bind(external_uid)
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load calendar event by external_uid")?;

    row.map(TryInto::try_into).transpose()
}

pub async fn upsert_from_caldav(
    pool: &SqlitePool,
    external_uid: &str,
    title: &str,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    all_day: bool,
    external_etag: Option<&str>,
    external_href: Option<&str>,
    now: DateTime<Utc>,
) -> Result<UpsertAction> {
    let existing = find_by_external_uid(pool, external_uid).await?;

    match existing {
        None => {
            let id = Uuid::new_v4();
            let nlreminder_owned = title.starts_with("TODO:");
            insert(
                pool,
                InsertCalendarEvent {
                    id,
                    external_uid,
                    title,
                    starts_at,
                    ends_at,
                    all_day,
                    external_etag,
                    external_href,
                    nlreminder_owned,
                    now,
                },
            )
            .await?;
            Ok(UpsertAction::Inserted)
        }
        Some(row) if !row.nlreminder_owned => {
            update_from_server(
                pool,
                row.id,
                title,
                starts_at,
                ends_at,
                all_day,
                external_etag,
                external_href,
                now,
            )
            .await?;
            Ok(UpsertAction::Updated)
        }
        Some(row) => {
            update_server_metadata(pool, row.id, external_etag, external_href, now).await?;
            Ok(UpsertAction::Updated)
        }
    }
}

pub async fn mark_in_range_missing_completed(
    pool: &SqlitePool,
    seen_external_uids: &[String],
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<u32> {
    let updated = if seen_external_uids.is_empty() {
        sqlx::query(
            r#"
            UPDATE calendar_events
            SET state = 'completed', updated_at = ?
            WHERE external_uid IS NOT NULL
              AND state IN ('scheduled', 'prepared')
              AND starts_at IS NOT NULL
              AND starts_at >= ?
              AND starts_at < ?
            "#,
        )
        .bind(format_dt(now))
        .bind(format_dt(range_start))
        .bind(format_dt(range_end))
        .execute(pool)
        .await?
        .rows_affected()
    } else {
        let placeholders = seen_external_uids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!(
            r#"
            UPDATE calendar_events
            SET state = 'completed', updated_at = ?
            WHERE external_uid IS NOT NULL
              AND state IN ('scheduled', 'prepared')
              AND starts_at IS NOT NULL
              AND starts_at >= ?
              AND starts_at < ?
              AND external_uid NOT IN ({placeholders})
            "#
        );
        let mut q = sqlx::query(&query)
            .bind(format_dt(now))
            .bind(format_dt(range_start))
            .bind(format_dt(range_end));
        for uid in seen_external_uids {
            q = q.bind(uid);
        }
        q.execute(pool).await?.rows_affected()
    };

    Ok(updated as u32)
}

struct InsertCalendarEvent<'a> {
    id: Uuid,
    external_uid: &'a str,
    title: &'a str,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    all_day: bool,
    external_etag: Option<&'a str>,
    external_href: Option<&'a str>,
    nlreminder_owned: bool,
    now: DateTime<Utc>,
}

async fn insert(pool: &SqlitePool, row: InsertCalendarEvent<'_>) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO calendar_events (
            id, external_uid, external_etag, external_href, title,
            starts_at, ends_at, all_day, state, remind_enabled,
            nlreminder_owned, source_todo_id, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'scheduled', 1, ?, NULL, ?, ?)
        "#,
    )
    .bind(row.id.to_string())
    .bind(row.external_uid)
    .bind(row.external_etag)
    .bind(row.external_href)
    .bind(row.title)
    .bind(opt_dt(row.starts_at))
    .bind(opt_dt(row.ends_at))
    .bind(bool_i64(row.all_day))
    .bind(bool_i64(row.nlreminder_owned))
    .bind(format_dt(row.now))
    .bind(format_dt(row.now))
    .execute(pool)
    .await
    .wrap_err("failed to insert calendar event")?;

    Ok(())
}

async fn update_from_server(
    pool: &SqlitePool,
    id: Uuid,
    title: &str,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    all_day: bool,
    external_etag: Option<&str>,
    external_href: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE calendar_events
        SET title = ?, starts_at = ?, ends_at = ?, all_day = ?,
            external_etag = ?, external_href = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(title)
    .bind(opt_dt(starts_at))
    .bind(opt_dt(ends_at))
    .bind(bool_i64(all_day))
    .bind(external_etag)
    .bind(external_href)
    .bind(format_dt(now))
    .bind(id.to_string())
    .execute(pool)
    .await
    .wrap_err("failed to update calendar event from server")?;

    Ok(())
}

async fn update_server_metadata(
    pool: &SqlitePool,
    id: Uuid,
    external_etag: Option<&str>,
    external_href: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE calendar_events
        SET external_etag = ?, external_href = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(external_etag)
    .bind(external_href)
    .bind(format_dt(now))
    .bind(id.to_string())
    .execute(pool)
    .await
    .wrap_err("failed to update calendar event metadata")?;

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct CalendarEventRow {
    id: String,
    external_uid: Option<String>,
    external_etag: Option<String>,
    external_href: Option<String>,
    title: String,
    starts_at: Option<String>,
    ends_at: Option<String>,
    all_day: i64,
    state: String,
    remind_enabled: i64,
    nlreminder_owned: i64,
    source_todo_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<CalendarEventRow> for CalendarEvent {
    type Error = color_eyre::Report;

    fn try_from(row: CalendarEventRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).wrap_err("invalid calendar event id")?,
            external_uid: row.external_uid,
            external_etag: row.external_etag,
            external_href: row.external_href,
            title: row.title,
            starts_at: row.starts_at.map(|v| parse_dt(&v)).transpose()?,
            ends_at: row.ends_at.map(|v| parse_dt(&v)).transpose()?,
            all_day: row.all_day != 0,
            state: CalendarEventState::parse(&row.state)?,
            remind_enabled: row.remind_enabled != 0,
            nlreminder_owned: row.nlreminder_owned != 0,
            source_todo_id: row
                .source_todo_id
                .map(|v| Uuid::parse_str(&v))
                .transpose()
                .wrap_err("invalid source_todo_id")?,
            created_at: parse_dt(&row.created_at)?,
            updated_at: parse_dt(&row.updated_at)?,
        })
    }
}

fn format_dt(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn opt_dt(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(format_dt)
}

fn parse_dt(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .wrap_err_with(|| format!("invalid RFC3339 datetime: {value}"))
}

fn bool_i64(value: bool) -> i64 {
    i64::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!(
            "nlreminder-models-test-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        db::connect_and_migrate(&path).await.unwrap()
    }

    #[tokio::test]
    async fn insert_and_find_by_external_uid() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        upsert_from_caldav(
            &pool,
            "uid-1",
            "Team meeting",
            Some(now),
            None,
            false,
            Some("etag-1"),
            Some("/events/1.ics"),
            now,
        )
        .await
        .unwrap();

        let event = find_by_external_uid(&pool, "uid-1")
            .await
            .unwrap()
            .expect("event");
        assert_eq!(event.title, "Team meeting");
        assert_eq!(event.external_etag.as_deref(), Some("etag-1"));
        assert!(!event.nlreminder_owned);
        assert_eq!(event.state, CalendarEventState::Scheduled);
    }

    #[tokio::test]
    async fn todo_prefix_marks_nlreminder_owned_on_insert() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        upsert_from_caldav(
            &pool,
            "uid-todo",
            "TODO: Report",
            Some(now),
            None,
            true,
            None,
            None,
            now,
        )
        .await
        .unwrap();

        let event = find_by_external_uid(&pool, "uid-todo").await.unwrap().unwrap();
        assert!(event.nlreminder_owned);
        assert!(event.all_day);
    }

    #[tokio::test]
    async fn server_owned_row_gets_full_update() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let later = now + chrono::Duration::hours(1);

        upsert_from_caldav(
            &pool, "uid-2", "Old title", Some(now), None, false, Some("e1"), None, now,
        )
        .await
        .unwrap();
        upsert_from_caldav(
            &pool,
            "uid-2",
            "New title",
            Some(later),
            None,
            false,
            Some("e2"),
            Some("/events/2.ics"),
            later,
        )
        .await
        .unwrap();

        let event = find_by_external_uid(&pool, "uid-2").await.unwrap().unwrap();
        assert_eq!(event.title, "New title");
        assert_eq!(event.starts_at, Some(later));
        assert_eq!(event.external_etag.as_deref(), Some("e2"));
    }

    #[tokio::test]
    async fn nlreminder_owned_row_keeps_title_on_resync() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let later = now + chrono::Duration::hours(2);

        upsert_from_caldav(
            &pool,
            "uid-owned",
            "TODO: Keep me",
            Some(now),
            None,
            true,
            Some("e1"),
            Some("/a.ics"),
            now,
        )
        .await
        .unwrap();
        upsert_from_caldav(
            &pool,
            "uid-owned",
            "TODO: Server changed",
            Some(later),
            None,
            true,
            Some("e2"),
            Some("/b.ics"),
            later,
        )
        .await
        .unwrap();

        let event = find_by_external_uid(&pool, "uid-owned")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.title, "TODO: Keep me");
        assert_eq!(event.starts_at, Some(now));
        assert_eq!(event.external_etag.as_deref(), Some("e2"));
        assert_eq!(event.external_href.as_deref(), Some("/b.ics"));
    }

    #[tokio::test]
    async fn marks_missing_events_completed_in_range() {
        let pool = test_pool().await;
        let start = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let end = start + chrono::Duration::days(7);
        let inside = start + chrono::Duration::hours(10);
        let now = start + chrono::Duration::days(1);

        upsert_from_caldav(
            &pool, "seen", "Seen", Some(inside), None, false, None, None, start,
        )
        .await
        .unwrap();
        upsert_from_caldav(
            &pool, "gone", "Gone", Some(inside), None, false, None, None, start,
        )
        .await
        .unwrap();

        let completed = super::mark_in_range_missing_completed(
            &pool,
            &["seen".to_owned()],
            start,
            end,
            now,
        )
        .await
        .unwrap();

        assert_eq!(completed, 1);
        let gone = find_by_external_uid(&pool, "gone").await.unwrap().unwrap();
        assert_eq!(gone.state, CalendarEventState::Completed);
        let seen = find_by_external_uid(&pool, "seen").await.unwrap().unwrap();
        assert_eq!(seen.state, CalendarEventState::Scheduled);
    }
}
