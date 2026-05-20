use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use sqlx::SqlitePool;

pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM sync_state WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .wrap_err("failed to read sync_state")?;

    Ok(row.map(|(value,)| value))
}

pub async fn set(pool: &SqlitePool, key: &str, value: &str, now: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO sync_state (key, value, updated_at)
        VALUES (?, ?, ?)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await
    .wrap_err("failed to write sync_state")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;
    use uuid::Uuid;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("nlreminder-sync-state-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn set_and_get_roundtrip() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        set(&pool, "google_tasks_updated_min", "2026-05-20T00:00:00+00:00", now)
            .await
            .unwrap();

        let value = get(&pool, "google_tasks_updated_min").await.unwrap();
        assert_eq!(value.as_deref(), Some("2026-05-20T00:00:00+00:00"));
    }
}
