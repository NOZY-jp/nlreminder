use std::collections::HashMap;

use crate::google::GmailMessage;

#[derive(Debug, Default)]
pub struct UnclassifiedMailQueue {
    by_gmail_id: HashMap<String, GmailMessage>,
}

impl UnclassifiedMailQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, messages: impl IntoIterator<Item = GmailMessage>) {
        for message in messages {
            self.by_gmail_id
                .insert(message.message_id.clone(), message);
        }
    }

    pub fn remove_by_rfc_message_id(&mut self, rfc_message_id: &str) {
        self.by_gmail_id
            .retain(|_, message| message.rfc_message_id != rfc_message_id);
    }

    pub fn messages(&self) -> impl Iterator<Item = &GmailMessage> {
        self.by_gmail_id.values()
    }

    pub fn len(&self) -> usize {
        self.by_gmail_id.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn sample_message(id: &str, rfc_id: &str) -> GmailMessage {
        GmailMessage {
            message_id: id.to_owned(),
            rfc_message_id: rfc_id.to_owned(),
            subject: Some("Subject".to_owned()),
            sender: None,
            snippet: "snippet".to_owned(),
            received_at: Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
            body_plain: None,
        }
    }

    #[test]
    fn enqueue_dedupes_by_gmail_id() {
        let mut queue = UnclassifiedMailQueue::new();
        queue.enqueue(vec![
            sample_message("g1", "<a@example.com>"),
            sample_message("g1", "<a@example.com>"),
        ]);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn remove_by_rfc_message_id() {
        let mut queue = UnclassifiedMailQueue::new();
        queue.enqueue(vec![sample_message("g1", "<a@example.com>")]);
        queue.remove_by_rfc_message_id("<a@example.com>");
        assert_eq!(queue.len(), 0);
    }
}
