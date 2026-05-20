use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailRecord {
    pub id: Uuid,
    pub message_id: String,
    pub subject: Option<String>,
    pub sender: Option<String>,
    pub received_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub async fn find_by_message_id(
    pool: &SqlitePool,
    message_id: &str,
) -> Result<Option<MailRecord>> {
    let row = sqlx::query_as::<_, MailRecordRow>(
        r#"
        SELECT id, message_id, subject, sender, received_at, created_at
        FROM mail_records
        WHERE message_id = ?
        "#,
    )
    .bind(message_id)
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load mail record by message_id")?;

    row.map(TryInto::try_into).transpose()
}

pub async fn insert(
    pool: &SqlitePool,
    message_id: &str,
    subject: Option<&str>,
    sender: Option<&str>,
    received_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<MailRecord> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO mail_records (id, message_id, subject, sender, received_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(message_id)
    .bind(subject)
    .bind(sender)
    .bind(format_dt(received_at))
    .bind(format_dt(now))
    .execute(pool)
    .await
    .wrap_err("failed to insert mail record")?;

    find_by_message_id(pool, message_id)
        .await?
        .ok_or_else(|| color_eyre::eyre::eyre!("inserted mail record not found"))
}

#[derive(Debug, sqlx::FromRow)]
struct MailRecordRow {
    id: String,
    message_id: String,
    subject: Option<String>,
    sender: Option<String>,
    received_at: String,
    created_at: String,
}

impl TryFrom<MailRecordRow> for MailRecord {
    type Error = color_eyre::Report;

    fn try_from(row: MailRecordRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).wrap_err("invalid mail record id")?,
            message_id: row.message_id,
            subject: row.subject,
            sender: row.sender,
            received_at: parse_dt(&row.received_at)?,
            created_at: parse_dt(&row.created_at)?,
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
        let dir = std::env::temp_dir().join(format!("nlreminder-mail-record-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn insert_and_find_by_message_id() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let received = now + chrono::Duration::hours(1);

        let record = insert(
            &pool,
            "<msg-1@example.com>",
            Some("Hello"),
            Some("boss@example.com"),
            received,
            now,
        )
        .await
        .unwrap();

        assert_eq!(record.message_id, "<msg-1@example.com>");
        assert_eq!(record.subject.as_deref(), Some("Hello"));

        let loaded = find_by_message_id(&pool, "<msg-1@example.com>")
            .await
            .unwrap()
            .expect("record");
        assert_eq!(loaded.id, record.id);
    }
}
