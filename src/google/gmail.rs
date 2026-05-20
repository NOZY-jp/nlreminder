use chrono::{DateTime, Datelike, NaiveDate, Utc};
use color_eyre::eyre::{Context, Result, eyre};
use reqwest::Client;
use reqwest::StatusCode;
use serde::Deserialize;

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
pub const INITIAL_LOOKBACK_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmailMessage {
    pub message_id: String,
    pub rfc_message_id: String,
    pub subject: Option<String>,
    pub sender: Option<String>,
    pub snippet: String,
    pub received_at: DateTime<Utc>,
    pub body_plain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryFetchResult {
    pub message_ids: Vec<String>,
    pub history_id: String,
}

#[derive(Debug, Deserialize)]
struct MessagesListResponse {
    #[serde(default)]
    messages: Vec<MessageRef>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageRef {
    id: String,
}

#[derive(Debug, Deserialize)]
struct HistoryListResponse {
    #[serde(default)]
    history: Vec<HistoryRecord>,
    #[serde(rename = "historyId")]
    history_id: Option<String>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoryRecord {
    #[serde(default, rename = "messagesAdded")]
    messages_added: Vec<HistoryMessageAdded>,
}

#[derive(Debug, Deserialize)]
struct HistoryMessageAdded {
    message: MessageRef,
}

#[derive(Debug, Deserialize)]
struct GmailProfileResponse {
    #[serde(rename = "historyId")]
    history_id: String,
}

#[derive(Debug, Deserialize)]
struct MessageResponse {
    id: String,
    #[serde(default)]
    snippet: String,
    #[serde(rename = "internalDate")]
    internal_date: Option<String>,
    payload: Option<MessagePayload>,
}

#[derive(Debug, Deserialize)]
struct MessagePayload {
    #[serde(default)]
    headers: Vec<MessageHeader>,
    body: Option<MessageBody>,
    #[serde(default)]
    parts: Vec<MessagePayload>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageHeader {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct MessageBody {
    data: Option<String>,
}

pub fn initial_sync_after_date(now: DateTime<Utc>) -> NaiveDate {
    (now - chrono::Duration::days(INITIAL_LOOKBACK_DAYS)).date_naive()
}

pub fn gmail_after_query(date: NaiveDate) -> String {
    format!("after:{}/{}/{}", date.year(), date.month(), date.day())
}

pub fn parse_message_response(body: &str) -> Result<GmailMessage> {
    let message: MessageResponse =
        serde_json::from_str(body).wrap_err("failed to parse Gmail message response")?;
    message_from_response(message)
}

pub fn parse_messages_list_response(body: &str) -> Result<Vec<String>> {
    let response: MessagesListResponse =
        serde_json::from_str(body).wrap_err("failed to parse Gmail messages.list response")?;

    Ok(response.messages.into_iter().map(|item| item.id).collect())
}

pub fn parse_history_list_response(body: &str) -> Result<HistoryFetchResult> {
    let response: HistoryListResponse =
        serde_json::from_str(body).wrap_err("failed to parse Gmail history.list response")?;

    let mut message_ids = Vec::new();
    for record in response.history {
        for added in record.messages_added {
            message_ids.push(added.message.id);
        }
    }

    let history_id = response
        .history_id
        .ok_or_else(|| eyre!("Gmail history.list response missing historyId"))?;

    Ok(HistoryFetchResult {
        message_ids,
        history_id,
    })
}

pub fn parse_profile_response(body: &str) -> Result<String> {
    let profile: GmailProfileResponse =
        serde_json::from_str(body).wrap_err("failed to parse Gmail profile response")?;
    Ok(profile.history_id)
}

pub async fn fetch_profile_history_id(access_token: &str) -> Result<String> {
    let http = Client::new();
    let response = http
        .get(format!("{GMAIL_API_BASE}/profile"))
        .bearer_auth(access_token)
        .send()
        .await
        .wrap_err("failed to call Gmail profile API")?
        .error_for_status()
        .wrap_err("Gmail profile API returned an error")?
        .text()
        .await
        .wrap_err("failed to read Gmail profile response body")?;

    parse_profile_response(&response)
}

pub async fn list_message_ids_for_query(
    access_token: &str,
    query: &str,
) -> Result<Vec<String>> {
    let http = Client::new();
    let mut ids = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut request = http
            .get(format!("{GMAIL_API_BASE}/messages"))
            .bearer_auth(access_token)
            .query(&[("q", query), ("maxResults", "100")]);

        if let Some(token) = &page_token {
            request = request.query(&[("pageToken", token.as_str())]);
        }

        let response = request
            .send()
            .await
            .wrap_err("failed to call Gmail messages.list API")?
            .error_for_status()
            .wrap_err("Gmail messages.list API returned an error")?
            .text()
            .await
            .wrap_err("failed to read Gmail messages.list response body")?;

        let page: MessagesListResponse = serde_json::from_str(&response)
            .wrap_err("failed to parse Gmail messages.list response")?;
        ids.extend(page.messages.into_iter().map(|item| item.id));
        page_token = page.next_page_token;

        if page_token.is_none() {
            break;
        }
    }

    Ok(ids)
}

pub async fn list_history_message_ids(
    access_token: &str,
    start_history_id: &str,
) -> Result<HistoryFetchResult> {
    let http = Client::new();
    let mut message_ids = Vec::new();
    let mut page_token: Option<String> = None;
    let mut latest_history_id = start_history_id.to_owned();

    loop {
        let mut request = http
            .get(format!("{GMAIL_API_BASE}/history"))
            .bearer_auth(access_token)
            .query(&[
                ("startHistoryId", start_history_id),
                ("historyTypes", "messageAdded"),
            ]);

        if let Some(token) = &page_token {
            request = request.query(&[("pageToken", token.as_str())]);
        }

        let response = request.send().await.wrap_err("failed to call Gmail history.list API")?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(eyre!("gmail history expired"));
        }

        let response = response
            .error_for_status()
            .wrap_err("Gmail history.list API returned an error")?
            .text()
            .await
            .wrap_err("failed to read Gmail history.list response body")?;

        let page: HistoryListResponse = serde_json::from_str(&response)
            .wrap_err("failed to parse Gmail history.list response")?;
        for record in page.history {
            for added in record.messages_added {
                message_ids.push(added.message.id);
            }
        }
        if let Some(history_id) = page.history_id {
            latest_history_id = history_id;
        }
        page_token = page.next_page_token;

        if page_token.is_none() {
            break;
        }
    }

    Ok(HistoryFetchResult {
        message_ids,
        history_id: latest_history_id,
    })
}

pub async fn get_message(access_token: &str, message_id: &str) -> Result<GmailMessage> {
    let http = Client::new();
    let response = http
        .get(format!("{GMAIL_API_BASE}/messages/{message_id}"))
        .query(&[("format", "full")])
        .bearer_auth(access_token)
        .send()
        .await
        .wrap_err_with(|| format!("failed to call Gmail messages.get API for {message_id}"))?
        .error_for_status()
        .wrap_err_with(|| format!("Gmail messages.get API returned an error for {message_id}"))?
        .text()
        .await
        .wrap_err("failed to read Gmail message response body")?;

    parse_message_response(&response)
}

fn message_from_response(message: MessageResponse) -> Result<GmailMessage> {
    let payload = message.payload.as_ref();
    let headers = payload.map(|part| part.headers.as_slice()).unwrap_or(&[]);

    let subject = header_value(headers, "Subject");
    let sender = header_value(headers, "From");
    let rfc_message_id = header_value(headers, "Message-ID")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("gmail:{}", message.id));

    let received_at = message
        .internal_date
        .as_deref()
        .map(parse_internal_date)
        .transpose()?
        .ok_or_else(|| eyre!("Gmail message {} missing internalDate", message.id))?;

    let body_plain = payload.and_then(extract_plain_body);

    Ok(GmailMessage {
        message_id: message.id,
        rfc_message_id,
        subject,
        sender,
        snippet: message.snippet,
        received_at,
        body_plain,
    })
}

fn header_value(headers: &[MessageHeader], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_internal_date(value: &str) -> Result<DateTime<Utc>> {
    let millis: i64 = value
        .parse()
        .wrap_err_with(|| format!("invalid Gmail internalDate: {value}"))?;
    DateTime::from_timestamp_millis(millis)
        .ok_or_else(|| eyre!("invalid Gmail internalDate millis: {value}"))
}

fn extract_plain_body(payload: &MessagePayload) -> Option<String> {
    if payload.mime_type.as_deref() == Some("text/plain") {
        if let Some(body) = payload.body.as_ref().and_then(|part| decode_body_data(&part.data)) {
            return Some(body);
        }
    }

    for part in &payload.parts {
        if let Some(body) = extract_plain_body(part) {
            return Some(body);
        }
    }

    None
}

fn decode_body_data(data: &Option<String>) -> Option<String> {
    use base64::Engine;

    let encoded = data.as_ref()?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()))
        .ok()?;
    String::from_utf8(decoded).ok()
}

pub fn dedupe_message_ids(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    #[test]
    fn gmail_after_query_formats_date() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        assert_eq!(gmail_after_query(date), "after:2026/5/14");
    }

    #[test]
    fn initial_sync_after_date_uses_seven_days() {
        let now = Utc.with_ymd_and_hms(2026, 5, 21, 12, 0, 0).unwrap();
        assert_eq!(
            initial_sync_after_date(now),
            NaiveDate::from_ymd_opt(2026, 5, 14).unwrap()
        );
    }

    #[test]
    fn parse_messages_list_response_extracts_ids() {
        let body = r#"{"messages":[{"id":"m1"},{"id":"m2"}],"nextPageToken":"t1"}"#;
        assert_eq!(
            parse_messages_list_response(body).unwrap(),
            vec!["m1".to_owned(), "m2".to_owned()]
        );
    }

    #[test]
    fn parse_history_list_response_extracts_added_messages() {
        let body = r#"{
            "history": [{
                "messagesAdded": [
                    {"message": {"id": "m1"}},
                    {"message": {"id": "m2"}}
                ]
            }],
            "historyId": "999"
        }"#;
        let result = parse_history_list_response(body).unwrap();
        assert_eq!(result.message_ids, vec!["m1", "m2"]);
        assert_eq!(result.history_id, "999");
    }

    #[test]
    fn parse_message_response_maps_headers_and_body() {
        let body = serde_json::json!({
            "id": "abc123",
            "snippet": "Hello there",
            "internalDate": "1716282000000",
            "payload": {
                "mimeType": "text/plain",
                "headers": [
                    {"name": "Subject", "value": "Report due"},
                    {"name": "From", "value": "boss@example.com"},
                    {"name": "Message-ID", "value": "<msg-1@example.com>"}
                ],
                "body": {
                    "data": "SGVsbG8gd29ybGQ="
                }
            }
        })
        .to_string();

        let message = parse_message_response(&body).unwrap();
        assert_eq!(message.message_id, "abc123");
        assert_eq!(message.rfc_message_id, "<msg-1@example.com>");
        assert_eq!(message.subject.as_deref(), Some("Report due"));
        assert_eq!(message.sender.as_deref(), Some("boss@example.com"));
        assert_eq!(message.body_plain.as_deref(), Some("Hello world"));
    }

    #[test]
    fn dedupe_message_ids_removes_duplicates() {
        assert_eq!(
            dedupe_message_ids(vec!["a".into(), "b".into(), "a".into()]),
            vec!["a", "b"]
        );
    }
}
