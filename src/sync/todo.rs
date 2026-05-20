use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use sqlx::SqlitePool;

use crate::caldav::CalDavClient;
use crate::config::{EnvConfig, Settings};
use crate::google::{GoogleTask, list_tasks, refresh_access_token, resolve_list_id};
use crate::models::{
    Todo, TodoUpsertAction, sync_state_get, sync_state_set, upsert_from_google_task,
};

use super::todo_calendar::sync_todo_deadline_to_caldav;

const SYNC_STATE_KEY: &str = "google_tasks_updated_min";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncTodoReport {
    pub upserted: u32,
    pub calendar_synced: u32,
}

pub async fn sync_google_tasks_to_db(
    pool: &SqlitePool,
    env: &EnvConfig,
    settings: &Settings,
    client: &CalDavClient,
) -> Result<SyncTodoReport> {
    sync_google_tasks_to_db_at(pool, env, settings, client, Utc::now()).await
}

pub async fn sync_google_tasks_to_db_at(
    pool: &SqlitePool,
    env: &EnvConfig,
    settings: &Settings,
    client: &CalDavClient,
    now: DateTime<Utc>,
) -> Result<SyncTodoReport> {
    let token = refresh_access_token(env).await?;
    let list_id = resolve_list_id(settings);
    let updated_min = match sync_state_get(pool, SYNC_STATE_KEY).await? {
        Some(value) => Some(parse_sync_timestamp(&value)?),
        None => None,
    };

    let tasks = list_tasks(&token.access_token, list_id, updated_min).await?;
    let (mut report, todos) = sync_fetched_google_tasks_to_db(pool, &tasks, now).await?;

    for todo in todos {
        if todo.due_at.is_some() && todo.state != crate::models::TodoState::Done {
            sync_todo_deadline_to_caldav(pool, client, settings, &todo, now).await?;
            report.calendar_synced += 1;
        }
    }

    if let Some(latest) = tasks.iter().map(|task| task.updated).max() {
        sync_state_set(pool, SYNC_STATE_KEY, &latest.to_rfc3339(), now).await?;
    }

    Ok(report)
}

pub async fn sync_fetched_google_tasks_to_db(
    pool: &SqlitePool,
    tasks: &[GoogleTask],
    now: DateTime<Utc>,
) -> Result<(SyncTodoReport, Vec<Todo>)> {
    let mut upserted = 0u32;
    let mut todos = Vec::with_capacity(tasks.len());

    for task in tasks {
        let (action, todo) = upsert_from_google_task(
            pool,
            &task.id,
            &task.title,
            task.due,
            task.completed,
            now,
        )
        .await?;
        if action == TodoUpsertAction::Inserted || action == TodoUpsertAction::Updated {
            upserted += 1;
        }
        todos.push(todo);
    }

    Ok((
        SyncTodoReport {
            upserted,
            calendar_synced: 0,
        },
        todos,
    ))
}

fn parse_sync_timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| color_eyre::eyre::eyre!("invalid sync timestamp {value}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;
    use crate::models::TodoState;
    use uuid::Uuid;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("nlreminder-sync-todo-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    fn sample_task(id: &str, title: &str, completed: bool) -> GoogleTask {
        GoogleTask {
            id: id.to_owned(),
            title: title.to_owned(),
            due: Some(Utc.with_ymd_and_hms(2026, 5, 22, 0, 0, 0).unwrap()),
            completed,
            updated: Utc.with_ymd_and_hms(2026, 5, 21, 12, 0, 0).unwrap(),
        }
    }

    #[tokio::test]
    async fn sync_fetched_google_tasks_upserts_todos() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let tasks = vec![
            sample_task("g1", "Task A", false),
            sample_task("g2", "Task B", true),
        ];

        let (report, todos) = sync_fetched_google_tasks_to_db(&pool, &tasks, now)
            .await
            .unwrap();

        assert_eq!(report.upserted, 2);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].state, TodoState::Todo);
        assert_eq!(todos[1].state, TodoState::Done);
    }
}
