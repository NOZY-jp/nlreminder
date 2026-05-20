use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;

use crate::caldav::{CalDavClient, PutCalendarEventRequest, put_calendar_event};
use crate::config::Settings;
use crate::google::all_day_range_for_due;
use crate::models::{Todo, TodoState, find_by_external_uid, todo_calendar_uid, upsert_from_todo_deadline};
use sqlx::SqlitePool;

pub async fn sync_todo_deadline_to_caldav(
    pool: &SqlitePool,
    client: &CalDavClient,
    settings: &Settings,
    todo: &Todo,
    now: DateTime<Utc>,
) -> Result<()> {
    let due_at = match todo.due_at {
        Some(value) => value,
        None => return Ok(()),
    };

    if todo.state == TodoState::Done {
        return Ok(());
    }

    let timezone = settings
        .timezone
        .parse()
        .map_err(|err| color_eyre::eyre::eyre!("invalid timezone {}: {err}", settings.timezone))?;
    let (starts_at, ends_at) = all_day_range_for_due(due_at, timezone)?;
    let uid = todo_calendar_uid(todo.id);
    let title = format!("TODO: {}", todo.title);
    let existing = find_by_external_uid(pool, &uid).await?;

    let response = put_calendar_event(
        client,
        settings,
        PutCalendarEventRequest {
            uid: uid.clone(),
            title: title.clone(),
            starts_at,
            ends_at,
            all_day: true,
            etag: existing
                .as_ref()
                .and_then(|row| row.external_etag.clone()),
            href: existing
                .as_ref()
                .and_then(|row| row.external_href.clone()),
            nlreminder_owned: true,
        },
    )
    .await?;

    upsert_from_todo_deadline(
        pool,
        todo.id,
        &uid,
        &title,
        starts_at,
        ends_at,
        &response.etag,
        &response.href,
        now,
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::caldav::CalDavClient;
    use crate::db;
    use crate::models::{CalendarEventState, upsert_from_google_task};
    use uuid::Uuid;

    async fn test_pool() -> SqlitePool {
        let dir =
            std::env::temp_dir().join(format!("nlreminder-sync-todo-cal-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn skips_todo_without_due_at() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let (_, todo) = upsert_from_google_task(
            &pool,
            "g-no-due",
            "No due",
            None,
            false,
            now,
        )
        .await
        .unwrap();

        let settings = test_settings();
        let client = CalDavClient::new(&test_env(), &settings).unwrap();

        sync_todo_deadline_to_caldav(&pool, &client, &settings, &todo, now)
            .await
            .unwrap();

        let uid = todo_calendar_uid(todo.id);
        let event = find_by_external_uid(&pool, &uid).await.unwrap();
        assert!(event.is_none());
    }

    fn test_env() -> crate::config::EnvConfig {
        crate::config::EnvConfig {
            google_client_id: String::new(),
            google_client_secret: String::new(),
            google_refresh_token: None,
            google_account_email: String::new(),
            lmstudio_model: String::new(),
            lmstudio_base_url: String::new(),
            caldav_url: "https://example.test/nozyjp/".to_owned(),
            caldav_username: "user".to_owned(),
            caldav_password: "pass".to_owned(),
            discord_token: String::new(),
            discord_guild_id: String::new(),
            discord_channel_id: String::new(),
            oauth_callback_port: 8080,
        }
    }

    fn test_settings() -> Settings {
        Settings {
            timezone: "Asia/Tokyo".to_owned(),
            morning_summary_hour: 8,
            quiet_hours_start: 0,
            quiet_hours_end: 8,
            scan_interval_secs: 300,
            database_path: "data/nlreminder.db".into(),
            backup_dir: "data/backups".into(),
            google_tasks_list_id: String::new(),
            caldav_calendar_path: "/nozyjp/sshCalendar/".to_owned(),
        }
    }

    #[tokio::test]
    async fn upsert_from_todo_deadline_links_source_todo() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let (_, todo) = upsert_from_google_task(
            &pool,
            "g-due",
            "Report",
            Some(Utc.with_ymd_and_hms(2026, 5, 22, 0, 0, 0).unwrap()),
            false,
            now,
        )
        .await
        .unwrap();
        let uid = todo_calendar_uid(todo.id);

        upsert_from_todo_deadline(
            &pool,
            todo.id,
            &uid,
            "TODO: Report",
            now,
            now + chrono::Duration::days(1),
            "etag-1",
            "https://example.test/event.ics",
            now,
        )
        .await
        .unwrap();

        let event = find_by_external_uid(&pool, &uid).await.unwrap().unwrap();
        assert_eq!(event.source_todo_id, Some(todo.id));
        assert!(event.nlreminder_owned);
        assert!(event.all_day);
        assert_eq!(event.state, CalendarEventState::Scheduled);
    }
}
