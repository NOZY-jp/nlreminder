use std::path::Path;
use std::str::FromStr;

use color_eyre::eyre::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

pub async fn connect_and_migrate(database_path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = database_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .wrap_err_with(|| format!("failed to create {}", parent.display()))?;
    }

    let options = SqliteConnectOptions::from_str(&format!(
        "sqlite:{}?mode=rwc",
        database_path.display()
    ))?
    .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .wrap_err_with(|| format!("failed to connect sqlite at {}", database_path.display()))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .wrap_err("failed to run database migrations")?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrate_creates_required_tables() {
        let dir = std::env::temp_dir().join(format!(
            "nlreminder-test-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let db_path = dir.join("test.db");

        let pool = connect_and_migrate(&db_path).await.unwrap();

        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let names: Vec<_> = tables.into_iter().map(|(name,)| name).collect();
        assert!(names.contains(&"calendar_events".to_owned()));
        assert!(names.contains(&"todos".to_owned()));
        assert!(names.contains(&"mail_records".to_owned()));
        assert!(names.contains(&"reminder_plans".to_owned()));
        assert!(names.contains(&"reminder_statuses".to_owned()));
        assert!(names.contains(&"llm_logs".to_owned()));
        assert!(names.contains(&"sync_state".to_owned()));

        let columns: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM pragma_table_info('calendar_events') ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let column_names: Vec<_> = columns.into_iter().map(|(name,)| name).collect();
        assert!(column_names.contains(&"external_href".to_owned()));
        assert!(column_names.contains(&"all_day".to_owned()));
        assert!(column_names.contains(&"nlreminder_owned".to_owned()));
        assert!(column_names.contains(&"source_todo_id".to_owned()));

        let _ = tokio::fs::remove_dir_all(dir).await;
    }
}
