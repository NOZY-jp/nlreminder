use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use color_eyre::eyre::{Context, Result, eyre};
use reqwest::Client;
use serde::Deserialize;

const TASKS_API_BASE: &str = "https://tasks.googleapis.com/tasks/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleTask {
    pub id: String,
    pub title: String,
    pub due: Option<DateTime<Utc>>,
    pub completed: bool,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct TasksListResponse {
    #[serde(default)]
    items: Vec<TaskItem>,
}

#[derive(Debug, Deserialize)]
struct TaskItem {
    id: String,
    #[serde(default)]
    title: String,
    due: Option<String>,
    #[serde(default = "default_status")]
    status: String,
    updated: String,
}

fn default_status() -> String {
    "needsAction".to_owned()
}

pub fn resolve_list_id(settings: &crate::config::Settings) -> &str {
    if settings.google_tasks_list_id.is_empty() {
        "@default"
    } else {
        &settings.google_tasks_list_id
    }
}

pub fn parse_tasks_list_response(body: &str) -> Result<Vec<GoogleTask>> {
    let response: TasksListResponse =
        serde_json::from_str(body).wrap_err("failed to parse Google Tasks list response")?;

    response
        .items
        .into_iter()
        .map(parse_task_item)
        .collect()
}

fn parse_task_item(item: TaskItem) -> Result<GoogleTask> {
    Ok(GoogleTask {
        id: item.id,
        title: item.title,
        due: item.due.map(|value| parse_rfc3339(&value)).transpose()?,
        completed: item.status == "completed",
        updated: parse_rfc3339(&item.updated)?,
    })
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .wrap_err_with(|| format!("invalid RFC3339 datetime: {value}"))
}

pub async fn list_tasks(
    access_token: &str,
    list_id: &str,
    updated_min: Option<DateTime<Utc>>,
) -> Result<Vec<GoogleTask>> {
    let http = Client::new();
    let url = format!("{TASKS_API_BASE}/lists/{list_id}/tasks");
    let mut query = vec![("showCompleted", "true"), ("showHidden", "true")];
    let updated_min_value;
    if let Some(min) = updated_min {
        updated_min_value = min.to_rfc3339();
        query.push(("updatedMin", &updated_min_value));
    }

    let response = http
        .get(&url)
        .query(&query)
        .bearer_auth(access_token)
        .send()
        .await
        .wrap_err("failed to call Google Tasks API")?
        .error_for_status()
        .wrap_err("Google Tasks API returned an error")?
        .text()
        .await
        .wrap_err("failed to read Google Tasks response body")?;

    parse_tasks_list_response(&response)
}

pub fn all_day_range_for_due(
    due_at: DateTime<Utc>,
    timezone: Tz,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let local_date = due_at.with_timezone(&timezone).date_naive();
    let start_local = local_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| eyre!("invalid due date"))?;
    let start = timezone
        .from_local_datetime(&start_local)
        .single()
        .ok_or_else(|| eyre!("ambiguous local start for due date"))?;
    let end = start + chrono::Duration::days(1);
    Ok((start.with_timezone(&Utc), end.with_timezone(&Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone as _};

    #[test]
    fn parse_tasks_list_response_maps_fields() {
        let body = r#"{
            "items": [
                {
                    "id": "task-1",
                    "title": "Report",
                    "status": "needsAction",
                    "updated": "2026-05-20T12:00:00.000Z",
                    "due": "2026-05-22T00:00:00.000Z"
                },
                {
                    "id": "task-2",
                    "title": "Done",
                    "status": "completed",
                    "updated": "2026-05-21T08:00:00.000Z"
                }
            ]
        }"#;

        let tasks = parse_tasks_list_response(body).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].title, "Report");
        assert!(tasks[0].due.is_some());
        assert!(!tasks[0].completed);
        assert!(tasks[1].completed);
    }

    #[test]
    fn resolve_list_id_uses_default_when_empty() {
        let settings = crate::config::Settings {
            timezone: "Asia/Tokyo".to_owned(),
            morning_summary_hour: 8,
            quiet_hours_start: 0,
            quiet_hours_end: 8,
            scan_interval_secs: 300,
            database_path: "data/nlreminder.db".into(),
            backup_dir: "data/backups".into(),
            google_tasks_list_id: String::new(),
            caldav_calendar_path: String::new(),
        };
        assert_eq!(resolve_list_id(&settings), "@default");
    }

    #[test]
    fn all_day_range_for_due_uses_local_date() {
        let tz: Tz = "Asia/Tokyo".parse().unwrap();
        let due = Utc.with_ymd_and_hms(2026, 5, 21, 15, 0, 0).unwrap();
        let (start, end) = all_day_range_for_due(due, tz).unwrap();
        assert_eq!(
            start.with_timezone(&tz).date_naive(),
            NaiveDate::from_ymd_opt(2026, 5, 22).unwrap()
        );
        assert_eq!(end - start, chrono::Duration::days(1));
    }
}
