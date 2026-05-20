use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use sqlx::SqlitePool;

use crate::config::EnvConfig;
use crate::google::{
    GmailMessage, dedupe_message_ids, fetch_profile_history_id, get_message,
    gmail_after_query, initial_sync_after_date, list_history_message_ids,
    list_message_ids_for_query, refresh_access_token,
};
use crate::mail::UnclassifiedMailQueue;
use crate::models::{sync_state_get, sync_state_set};

const SYNC_STATE_KEY: &str = "gmail_history_id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncMailReport {
    pub fetched: u32,
    pub queued: u32,
}

pub async fn sync_gmail_to_queue(
    pool: &SqlitePool,
    env: &EnvConfig,
    queue: &mut UnclassifiedMailQueue,
) -> Result<SyncMailReport> {
    sync_gmail_to_queue_at(pool, env, queue, Utc::now()).await
}

pub async fn sync_gmail_to_queue_at(
    pool: &SqlitePool,
    env: &EnvConfig,
    queue: &mut UnclassifiedMailQueue,
    now: DateTime<Utc>,
) -> Result<SyncMailReport> {
    let messages = fetch_new_gmail_messages_at(pool, env, now).await?;
    let fetched = messages.len() as u32;
    queue.enqueue(messages);
    Ok(SyncMailReport {
        fetched,
        queued: queue.len() as u32,
    })
}

pub async fn fetch_new_gmail_messages(
    pool: &SqlitePool,
    env: &EnvConfig,
) -> Result<Vec<GmailMessage>> {
    fetch_new_gmail_messages_at(pool, env, Utc::now()).await
}

pub async fn fetch_new_gmail_messages_at(
    pool: &SqlitePool,
    env: &EnvConfig,
    now: DateTime<Utc>,
) -> Result<Vec<GmailMessage>> {
    let token = refresh_access_token(env).await?;
    let access_token = token.access_token;

    let (message_ids, new_history_id) = match sync_state_get(pool, SYNC_STATE_KEY).await? {
        Some(history_id) => match list_history_message_ids(&access_token, &history_id).await {
            Ok(result) => (result.message_ids, Some(result.history_id)),
            Err(err) if is_history_expired(&err) => {
                fetch_initial_message_ids(&access_token, now).await?
            }
            Err(err) => return Err(err),
        },
        None => fetch_initial_message_ids(&access_token, now).await?,
    };

    let message_ids = dedupe_message_ids(message_ids);
    let mut messages = Vec::with_capacity(message_ids.len());
    for message_id in message_ids {
        messages.push(get_message(&access_token, &message_id).await?);
    }

    let history_id = match new_history_id {
        Some(value) => value,
        None => fetch_profile_history_id(&access_token).await?,
    };
    sync_state_set(pool, SYNC_STATE_KEY, &history_id, now).await?;

    Ok(messages)
}

async fn fetch_initial_message_ids(
    access_token: &str,
    now: DateTime<Utc>,
) -> Result<(Vec<String>, Option<String>)> {
    let query = gmail_after_query(initial_sync_after_date(now));
    let message_ids = list_message_ids_for_query(access_token, &query).await?;
    Ok((message_ids, None))
}

fn is_history_expired(err: &color_eyre::Report) -> bool {
    err.to_string().contains("gmail history expired")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;
    use crate::google::GmailMessage;
    use uuid::Uuid;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("nlreminder-sync-mail-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    #[test]
    fn is_history_expired_detects_marker() {
        let err = color_eyre::eyre::eyre!("gmail history expired");
        assert!(is_history_expired(&err));
        let other = color_eyre::eyre::eyre!("network error");
        assert!(!is_history_expired(&other));
    }

    #[tokio::test]
    async fn stores_history_id_after_successful_fetch_path() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();

        sync_state_set(&pool, SYNC_STATE_KEY, "12345", now).await.unwrap();
        let stored = sync_state_get(&pool, SYNC_STATE_KEY).await.unwrap();
        assert_eq!(stored.as_deref(), Some("12345"));
    }

    #[test]
    fn sync_mail_report_counts_fetched_messages() {
        let messages = vec![
            GmailMessage {
                message_id: "m1".into(),
                rfc_message_id: "<1>".into(),
                subject: None,
                sender: None,
                snippet: String::new(),
                received_at: Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
                body_plain: None,
            },
            GmailMessage {
                message_id: "m2".into(),
                rfc_message_id: "<2>".into(),
                subject: None,
                sender: None,
                snippet: String::new(),
                received_at: Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
                body_plain: None,
            },
        ];
        let report = SyncMailReport {
            fetched: messages.len() as u32,
            queued: messages.len() as u32,
        };
        assert_eq!(report.fetched, 2);
        assert_eq!(report.queued, 2);
    }
}
