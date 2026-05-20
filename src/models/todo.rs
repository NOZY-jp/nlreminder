use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::enums::TodoState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    pub id: Uuid,
    pub external_task_id: Option<String>,
    pub title: String,
    pub due_at: Option<DateTime<Utc>>,
    pub state: TodoState,
    pub remind_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertAction {
    Inserted,
    Updated,
}

pub fn todo_calendar_uid(todo_id: Uuid) -> String {
    format!("nlreminder-todo-{todo_id}")
}

pub async fn find_by_external_task_id(
    pool: &SqlitePool,
    external_task_id: &str,
) -> Result<Option<Todo>> {
    let row = sqlx::query_as::<_, TodoRow>(
        r#"
        SELECT id, external_task_id, title, due_at, state, remind_enabled, created_at, updated_at
        FROM todos
        WHERE external_task_id = ?
        "#,
    )
    .bind(external_task_id)
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load todo by external_task_id")?;

    row.map(TryInto::try_into).transpose()
}

pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Todo>> {
    let row = sqlx::query_as::<_, TodoRow>(
        r#"
        SELECT id, external_task_id, title, due_at, state, remind_enabled, created_at, updated_at
        FROM todos
        WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .wrap_err("failed to load todo by id")?;

    row.map(TryInto::try_into).transpose()
}

pub async fn list_open(pool: &SqlitePool) -> Result<Vec<Todo>> {
    let rows = sqlx::query_as::<_, TodoRow>(
        r#"
        SELECT id, external_task_id, title, due_at, state, remind_enabled, created_at, updated_at
        FROM todos
        WHERE state IN ('todo', 'ongoing')
        ORDER BY due_at IS NULL, due_at ASC, title ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .wrap_err("failed to list open todos")?;

    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn upsert_from_google_task(
    pool: &SqlitePool,
    external_task_id: &str,
    title: &str,
    due_at: Option<DateTime<Utc>>,
    completed: bool,
    now: DateTime<Utc>,
) -> Result<(UpsertAction, Todo)> {
    let existing = find_by_external_task_id(pool, external_task_id).await?;

    let state = if completed {
        TodoState::Done
    } else {
        match existing.as_ref().map(|row| row.state) {
            Some(TodoState::Ongoing) => TodoState::Ongoing,
            _ => TodoState::Todo,
        }
    };

    match existing {
        None => {
            let id = Uuid::new_v4();
            insert(
                pool,
                InsertTodo {
                    id,
                    external_task_id,
                    title,
                    due_at,
                    state,
                    now,
                },
            )
            .await?;
            let todo = find_by_external_task_id(pool, external_task_id)
                .await?
                .expect("inserted todo");
            Ok((UpsertAction::Inserted, todo))
        }
        Some(row) => {
            update(
                pool,
                row.id,
                title,
                due_at,
                state,
                now,
            )
            .await?;
            let todo = find_by_external_task_id(pool, external_task_id)
                .await?
                .expect("updated todo");
            Ok((UpsertAction::Updated, todo))
        }
    }
}

struct InsertTodo<'a> {
    id: Uuid,
    external_task_id: &'a str,
    title: &'a str,
    due_at: Option<DateTime<Utc>>,
    state: TodoState,
    now: DateTime<Utc>,
}

async fn insert(pool: &SqlitePool, row: InsertTodo<'_>) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO todos (
            id, external_task_id, title, due_at, state, remind_enabled, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, 1, ?, ?)
        "#,
    )
    .bind(row.id.to_string())
    .bind(row.external_task_id)
    .bind(row.title)
    .bind(opt_dt(row.due_at))
    .bind(row.state.as_str())
    .bind(format_dt(row.now))
    .bind(format_dt(row.now))
    .execute(pool)
    .await
    .wrap_err("failed to insert todo")?;

    Ok(())
}

async fn update(
    pool: &SqlitePool,
    id: Uuid,
    title: &str,
    due_at: Option<DateTime<Utc>>,
    state: TodoState,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE todos
        SET title = ?, due_at = ?, state = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(title)
    .bind(opt_dt(due_at))
    .bind(state.as_str())
    .bind(format_dt(now))
    .bind(id.to_string())
    .execute(pool)
    .await
    .wrap_err("failed to update todo")?;

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct TodoRow {
    id: String,
    external_task_id: Option<String>,
    title: String,
    due_at: Option<String>,
    state: String,
    remind_enabled: i64,
    created_at: String,
    updated_at: String,
}

impl TryFrom<TodoRow> for Todo {
    type Error = color_eyre::Report;

    fn try_from(row: TodoRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).wrap_err("invalid todo id")?,
            external_task_id: row.external_task_id,
            title: row.title,
            due_at: row.due_at.map(|v| parse_dt(&v)).transpose()?,
            state: TodoState::parse(&row.state)?,
            remind_enabled: row.remind_enabled != 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("nlreminder-todo-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[test]
    fn todo_calendar_uid_is_stable() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            todo_calendar_uid(id),
            "nlreminder-todo-550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[tokio::test]
    async fn insert_and_find_by_external_task_id() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let due = Utc.with_ymd_and_hms(2026, 5, 22, 0, 0, 0).unwrap();

        let (action, todo) = upsert_from_google_task(
            &pool,
            "google-task-1",
            "Report",
            Some(due),
            false,
            now,
        )
        .await
        .unwrap();

        assert_eq!(action, UpsertAction::Inserted);
        assert_eq!(todo.title, "Report");
        assert_eq!(todo.due_at, Some(due));
        assert_eq!(todo.state, TodoState::Todo);
    }

    #[tokio::test]
    async fn completed_task_maps_to_done() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        upsert_from_google_task(&pool, "google-task-2", "Done task", None, false, now)
            .await
            .unwrap();
        let (_, todo) = upsert_from_google_task(
            &pool,
            "google-task-2",
            "Done task",
            None,
            true,
            now,
        )
        .await
        .unwrap();

        assert_eq!(todo.state, TodoState::Done);
    }

    #[tokio::test]
    async fn preserves_ongoing_state_on_resync() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        let (action, todo) = upsert_from_google_task(
            &pool,
            "google-task-3",
            "In progress",
            None,
            false,
            now,
        )
        .await
        .unwrap();
        assert_eq!(action, UpsertAction::Inserted);

        sqlx::query("UPDATE todos SET state = 'ongoing' WHERE id = ?")
            .bind(todo.id.to_string())
            .execute(&pool)
            .await
            .unwrap();

        let (_, todo) = upsert_from_google_task(
            &pool,
            "google-task-3",
            "In progress",
            None,
            false,
            now,
        )
        .await
        .unwrap();

        assert_eq!(todo.state, TodoState::Ongoing);
    }

    #[tokio::test]
    async fn list_open_excludes_done_todos() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        upsert_from_google_task(&pool, "open-1", "Open", None, false, now)
            .await
            .unwrap();
        upsert_from_google_task(&pool, "done-1", "Done", None, true, now)
            .await
            .unwrap();

        let open = list_open(&pool).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].title, "Open");
    }
}
