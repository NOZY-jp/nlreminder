use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::config::Settings;
use crate::schedule::adjust_for_quiet_hours_at;

use super::enums::{ReminderPlanStatus, ReminderTargetType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderPlan {
    pub id: Uuid,
    pub target_type: ReminderTargetType,
    pub target_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    pub status: ReminderPlanStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<ReminderPlan>> {
    let row = sqlx::query_as::<_, ReminderPlanRow>(
        r#"
        SELECT id, target_type, target_id, scheduled_at, status, created_at, updated_at
        FROM reminder_plans
        WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load reminder plan by id")?;

    row.map(TryInto::try_into).transpose()
}

pub async fn get_active(
    pool: &SqlitePool,
    target_type: ReminderTargetType,
    target_id: Uuid,
) -> Result<Option<ReminderPlan>> {
    let row = sqlx::query_as::<_, ReminderPlanRow>(
        r#"
        SELECT id, target_type, target_id, scheduled_at, status, created_at, updated_at
        FROM reminder_plans
        WHERE target_type = ? AND target_id = ? AND status = 'active'
        "#,
    )
    .bind(target_type.as_str())
    .bind(target_id.to_string())
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load active reminder plan")?;

    row.map(TryInto::try_into).transpose()
}

pub async fn list_due(
    pool: &SqlitePool,
    before: DateTime<Utc>,
) -> Result<Vec<ReminderPlan>> {
    let rows = sqlx::query_as::<_, ReminderPlanRow>(
        r#"
        SELECT id, target_type, target_id, scheduled_at, status, created_at, updated_at
        FROM reminder_plans
        WHERE status = 'active' AND scheduled_at <= ?
        ORDER BY scheduled_at ASC
        "#,
    )
    .bind(format_dt(before))
    .fetch_all(pool)
    .await
    .wrap_err("failed to list due reminder plans")?;

    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn create(
    pool: &SqlitePool,
    target_type: ReminderTargetType,
    target_id: Uuid,
    scheduled_at: DateTime<Utc>,
    settings: &Settings,
    now: DateTime<Utc>,
) -> Result<ReminderPlan> {
    cancel_active_for_target(pool, target_type, target_id, now).await?;

    let scheduled_at = adjust_for_quiet_hours_at(scheduled_at, settings)?;
    let id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO reminder_plans (
            id, target_type, target_id, scheduled_at, status, created_at, updated_at
        ) VALUES (?, ?, ?, ?, 'active', ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(target_type.as_str())
    .bind(target_id.to_string())
    .bind(format_dt(scheduled_at))
    .bind(format_dt(now))
    .bind(format_dt(now))
    .execute(pool)
    .await
    .wrap_err("failed to insert reminder plan")?;

    find_by_id(pool, id)
        .await?
        .ok_or_else(|| color_eyre::eyre::eyre!("inserted reminder plan not found"))
}

pub async fn cancel(pool: &SqlitePool, id: Uuid, now: DateTime<Utc>) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE reminder_plans
        SET status = 'cancelled', updated_at = ?
        WHERE id = ? AND status = 'active'
        "#,
    )
    .bind(format_dt(now))
    .bind(id.to_string())
    .execute(pool)
    .await
    .wrap_err("failed to cancel reminder plan")?
    .rows_affected();

    if updated == 0 {
        return Err(color_eyre::eyre::eyre!("reminder plan not found or not active: {id}"));
    }

    Ok(())
}

async fn cancel_active_for_target(
    pool: &SqlitePool,
    target_type: ReminderTargetType,
    target_id: Uuid,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE reminder_plans
        SET status = 'cancelled', updated_at = ?
        WHERE target_type = ? AND target_id = ? AND status = 'active'
        "#,
    )
    .bind(format_dt(now))
    .bind(target_type.as_str())
    .bind(target_id.to_string())
    .execute(pool)
    .await
    .wrap_err("failed to cancel active reminder plans for target")?;

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct ReminderPlanRow {
    id: String,
    target_type: String,
    target_id: String,
    scheduled_at: String,
    status: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ReminderPlanRow> for ReminderPlan {
    type Error = color_eyre::Report;

    fn try_from(row: ReminderPlanRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).wrap_err("invalid reminder plan id")?,
            target_type: ReminderTargetType::parse(&row.target_type)?,
            target_id: Uuid::parse_str(&row.target_id).wrap_err("invalid target id")?,
            scheduled_at: parse_dt(&row.scheduled_at)?,
            status: ReminderPlanStatus::parse(&row.status)?,
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
    use chrono::{TimeZone as _, Timelike};
    use chrono_tz::Asia::Tokyo;
    use crate::db;

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

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("nlreminder-reminder-plan-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn create_replaces_existing_active_plan() {
        let pool = test_pool().await;
        let target_id = Uuid::new_v4();
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 12, 0, 0).unwrap();
        let first_at = now + chrono::Duration::hours(2);
        let second_at = now + chrono::Duration::hours(4);

        let first = create(
            &pool,
            ReminderTargetType::Todo,
            target_id,
            first_at,
            &settings(),
            now,
        )
        .await
        .unwrap();
        let second = create(
            &pool,
            ReminderTargetType::Todo,
            target_id,
            second_at,
            &settings(),
            now,
        )
        .await
        .unwrap();

        assert_ne!(first.id, second.id);
        let active = get_active(&pool, ReminderTargetType::Todo, target_id)
            .await
            .unwrap()
            .expect("active plan");
        assert_eq!(active.id, second.id);

        let cancelled = find_by_id(&pool, first.id).await.unwrap().unwrap();
        assert_eq!(cancelled.status, ReminderPlanStatus::Cancelled);
    }

    #[tokio::test]
    async fn create_adjusts_quiet_hours() {
        let pool = test_pool().await;
        let target_id = Uuid::new_v4();
        let scheduled = Tokyo.with_ymd_and_hms(2026, 5, 21, 3, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        let plan = create(
            &pool,
            ReminderTargetType::CalendarEvent,
            target_id,
            scheduled.with_timezone(&Utc),
            &settings(),
            now,
        )
        .await
        .unwrap();

        assert_eq!(plan.scheduled_at.with_timezone(&Tokyo).hour(), 8);
    }

    #[tokio::test]
    async fn list_due_returns_active_plans_before_cutoff() {
        let pool = test_pool().await;
        let target_id = Uuid::new_v4();
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 12, 0, 0).unwrap();
        let due_at = now - chrono::Duration::minutes(5);
        let future_at = now + chrono::Duration::hours(1);

        create(
            &pool,
            ReminderTargetType::Todo,
            target_id,
            due_at,
            &settings(),
            now,
        )
        .await
        .unwrap();
        create(
            &pool,
            ReminderTargetType::Todo,
            Uuid::new_v4(),
            future_at,
            &settings(),
            now,
        )
        .await
        .unwrap();

        let due = list_due(&pool, now).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].target_id, target_id);
    }
}
