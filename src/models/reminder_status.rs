use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::enums::ReminderReaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderStatus {
    pub id: Uuid,
    pub plan_id: Option<Uuid>,
    pub discord_message_id: Option<String>,
    pub reaction: ReminderReaction,
    pub notified_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<ReminderStatus>> {
    let row = sqlx::query_as::<_, ReminderStatusRow>(
        r#"
        SELECT id, plan_id, discord_message_id, reaction, notified_at,
               acknowledged_at, created_at, updated_at
        FROM reminder_statuses
        WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load reminder status by id")?;

    row.map(TryInto::try_into).transpose()
}

pub async fn record_notification(
    pool: &SqlitePool,
    plan_id: Option<Uuid>,
    discord_message_id: Option<&str>,
    now: DateTime<Utc>,
) -> Result<ReminderStatus> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO reminder_statuses (
            id, plan_id, discord_message_id, reaction, notified_at,
            acknowledged_at, created_at, updated_at
        ) VALUES (?, ?, ?, 'no_response', ?, NULL, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(plan_id.map(|value| value.to_string()))
    .bind(discord_message_id)
    .bind(format_dt(now))
    .bind(format_dt(now))
    .bind(format_dt(now))
    .execute(pool)
    .await
    .wrap_err("failed to insert reminder status")?;

    find_by_id(pool, id)
        .await?
        .ok_or_else(|| color_eyre::eyre::eyre!("inserted reminder status not found"))
}

pub async fn list_no_response(
    pool: &SqlitePool,
    since_hours: i64,
    now: DateTime<Utc>,
) -> Result<Vec<ReminderStatus>> {
    let since = now - chrono::Duration::hours(since_hours);
    let rows = sqlx::query_as::<_, ReminderStatusRow>(
        r#"
        SELECT id, plan_id, discord_message_id, reaction, notified_at,
               acknowledged_at, created_at, updated_at
        FROM reminder_statuses
        WHERE reaction = 'no_response' AND notified_at >= ?
        ORDER BY notified_at ASC
        "#,
    )
    .bind(format_dt(since))
    .fetch_all(pool)
    .await
    .wrap_err("failed to list no-response reminder statuses")?;

    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn acknowledge(pool: &SqlitePool, id: Uuid, now: DateTime<Utc>) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE reminder_statuses
        SET reaction = 'acknowledged', acknowledged_at = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(format_dt(now))
    .bind(format_dt(now))
    .bind(id.to_string())
    .execute(pool)
    .await
    .wrap_err("failed to acknowledge reminder status")?
    .rows_affected();

    if updated == 0 {
        return Err(color_eyre::eyre::eyre!("reminder status not found: {id}"));
    }

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct ReminderStatusRow {
    id: String,
    plan_id: Option<String>,
    discord_message_id: Option<String>,
    reaction: String,
    notified_at: String,
    acknowledged_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ReminderStatusRow> for ReminderStatus {
    type Error = color_eyre::Report;

    fn try_from(row: ReminderStatusRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).wrap_err("invalid reminder status id")?,
            plan_id: row
                .plan_id
                .map(|value| Uuid::parse_str(&value))
                .transpose()
                .wrap_err("invalid plan id")?,
            discord_message_id: row.discord_message_id,
            reaction: ReminderReaction::parse(&row.reaction)?,
            notified_at: parse_dt(&row.notified_at)?,
            acknowledged_at: row
                .acknowledged_at
                .map(|value| parse_dt(&value))
                .transpose()?,
            created_at: parse_dt(&row.created_at)?,
            updated_at: parse_dt(&row.updated_at)?,
        })
    }
}

fn format_dt(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn parse_dt(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .wrap_err_with(|| format!("invalid RFC3339 datetime: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;

    async fn test_pool() -> SqlitePool {
        let dir =
            std::env::temp_dir().join(format!("nlreminder-reminder-status-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn record_and_acknowledge_status() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 12, 0, 0).unwrap();

        let status = record_notification(&pool, None, Some("discord-1"), now)
            .await
            .unwrap();
        assert_eq!(status.reaction, ReminderReaction::NoResponse);

        acknowledge(&pool, status.id, now + chrono::Duration::minutes(1))
            .await
            .unwrap();

        let updated = find_by_id(&pool, status.id).await.unwrap().unwrap();
        assert_eq!(updated.reaction, ReminderReaction::Acknowledged);
        assert!(updated.acknowledged_at.is_some());
    }

    #[tokio::test]
    async fn list_no_response_filters_by_since_hours() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 12, 0, 0).unwrap();
        let old = now - chrono::Duration::hours(30);

        record_notification(&pool, None, Some("recent"), now).await.unwrap();
        record_notification(&pool, None, Some("old"), old).await.unwrap();

        let statuses = list_no_response(&pool, 24, now).await.unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].discord_message_id.as_deref(), Some("recent"));
    }
}
