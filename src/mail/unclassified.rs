use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::google::GmailMessage;
use crate::models::{find_by_message_id, insert_mail_record};

use super::queue::UnclassifiedMailQueue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailMessageView {
    pub message_id: String,
    pub rfc_message_id: String,
    pub subject: Option<String>,
    pub sender: Option<String>,
    pub snippet: String,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistMailRequest {
    pub message_id: String,
    pub subject: Option<String>,
    pub sender: Option<String>,
    pub received_at: DateTime<Utc>,
}

pub async fn list_unclassified(
    pool: &SqlitePool,
    queue: &UnclassifiedMailQueue,
    limit: usize,
) -> Result<Vec<MailMessageView>> {
    let mut views = Vec::new();
    for message in queue.messages() {
        if views.len() >= limit {
            break;
        }
        if find_by_message_id(pool, &message.rfc_message_id)
            .await?
            .is_some()
        {
            continue;
        }
        views.push(to_view(message));
    }
    Ok(views)
}

pub async fn persist(
    pool: &SqlitePool,
    queue: &mut UnclassifiedMailQueue,
    request: PersistMailRequest,
    now: DateTime<Utc>,
) -> Result<Uuid> {
    let record = insert_mail_record(
        pool,
        &request.message_id,
        request.subject.as_deref(),
        request.sender.as_deref(),
        request.received_at,
        now,
    )
    .await?;
    queue.remove_by_rfc_message_id(&request.message_id);
    Ok(record.id)
}

fn to_view(message: &GmailMessage) -> MailMessageView {
    MailMessageView {
        message_id: message.message_id.clone(),
        rfc_message_id: message.rfc_message_id.clone(),
        subject: message.subject.clone(),
        sender: message.sender.clone(),
        snippet: message.snippet.clone(),
        received_at: message.received_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use crate::db;
    use crate::google::GmailMessage;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("nlreminder-mail-unclass-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        db::connect_and_migrate(&dir.join("test.db")).await.unwrap()
    }

    fn sample_message(id: &str, rfc_id: &str) -> GmailMessage {
        GmailMessage {
            message_id: id.to_owned(),
            rfc_message_id: rfc_id.to_owned(),
            subject: Some("Report".to_owned()),
            sender: Some("boss@example.com".to_owned()),
            snippet: "Please review".to_owned(),
            received_at: Utc.with_ymd_and_hms(2026, 5, 21, 9, 0, 0).unwrap(),
            body_plain: None,
        }
    }

    #[tokio::test]
    async fn list_unclassified_skips_persisted_messages() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let mut queue = UnclassifiedMailQueue::new();
        queue.enqueue(vec![
            sample_message("g1", "<a@example.com>"),
            sample_message("g2", "<b@example.com>"),
        ]);

        insert_mail_record(
            &pool,
            "<a@example.com>",
            Some("Report"),
            None,
            now,
            now,
        )
        .await
        .unwrap();

        let views = list_unclassified(&pool, &queue, 20).await.unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].rfc_message_id, "<b@example.com>");
    }

    #[tokio::test]
    async fn persist_inserts_and_removes_from_queue() {
        let pool = test_pool().await;
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
        let mut queue = UnclassifiedMailQueue::new();
        queue.enqueue(vec![sample_message("g1", "<a@example.com>")]);

        let id = persist(
            &pool,
            &mut queue,
            PersistMailRequest {
                message_id: "<a@example.com>".to_owned(),
                subject: Some("Report".to_owned()),
                sender: Some("boss@example.com".to_owned()),
                received_at: now,
            },
            now,
        )
        .await
        .unwrap();

        assert!(find_by_message_id(&pool, "<a@example.com>")
            .await
            .unwrap()
            .is_some());
        assert_eq!(queue.len(), 0);
        assert!(!id.is_nil());
    }
}
