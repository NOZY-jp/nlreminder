use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::enums::LlmCallType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmLog {
    pub id: Uuid,
    pub call_type: LlmCallType,
    pub model: String,
    pub input_summary: String,
    pub output_json: String,
    pub duration_ms: i64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct NewLlmLog<'a> {
    pub call_type: LlmCallType,
    pub model: &'a str,
    pub input_summary: &'a str,
    pub output_json: &'a str,
    pub duration_ms: i64,
    pub error: Option<&'a str>,
}

pub async fn insert(pool: &SqlitePool, row: NewLlmLog<'_>, now: DateTime<Utc>) -> Result<LlmLog> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO llm_logs (
            id, call_type, model, input_summary, output_json, duration_ms, error, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(row.call_type.as_str())
    .bind(row.model)
    .bind(row.input_summary)
    .bind(row.output_json)
    .bind(row.duration_ms)
    .bind(row.error)
    .bind(format_dt(now))
    .execute(pool)
    .await
    .wrap_err("failed to insert llm log")?;

    find_by_id(pool, id)
        .await?
        .ok_or_else(|| color_eyre::eyre::eyre!("inserted llm log not found"))
}

pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<LlmLog>> {
    let row = sqlx::query_as::<_, LlmLogRow>(
        r#"
        SELECT id, call_type, model, input_summary, output_json, duration_ms, error, created_at
        FROM llm_logs
        WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load llm log by id")?;

    row.map(TryInto::try_into).transpose()
}

#[derive(Debug, sqlx::FromRow)]
struct LlmLogRow {
    id: String,
    call_type: String,
    model: String,
    input_summary: String,
    output_json: String,
    duration_ms: i64,
    error: Option<String>,
    created_at: String,
}

impl TryFrom<LlmLogRow> for LlmLog {
    type Error = color_eyre::Report;

    fn try_from(row: LlmLogRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).wrap_err("invalid llm log id")?,
            call_type: LlmCallType::parse(&row.call_type)?,
            model: row.model,
            input_summary: row.input_summary,
            output_json: row.output_json,
            duration_ms: row.duration_ms,
            error: row.error,
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
        let dir = std::env::temp_dir().join(format!("nlreminder-llm-log-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn insert_and_find_llm_log() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        let log = insert(
            &pool,
            NewLlmLog {
                call_type: LlmCallType::MailClassify,
                model: "test-model",
                input_summary: "subject: hello",
                output_json: r#"{"persist":false}"#,
                duration_ms: 42,
                error: None,
            },
            now,
        )
        .await
        .unwrap();

        assert_eq!(log.call_type, LlmCallType::MailClassify);
        assert_eq!(log.duration_ms, 42);
    }
}
