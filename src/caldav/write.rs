use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use color_eyre::eyre::{Context, Result, eyre};
use url::Url;

use crate::config::Settings;

use super::client::CalDavClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutCalendarEventRequest {
    pub uid: String,
    pub title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
    pub etag: Option<String>,
    pub href: Option<String>,
    pub nlreminder_owned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutCalendarEventResponse {
    pub href: String,
    pub etag: String,
    pub uid: String,
}

pub fn build_all_day_event_ics(
    uid: &str,
    title: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//nlreminder//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         SUMMARY:{title}\r\n\
         DTSTART;VALUE=DATE:{start}\r\n\
         DTEND;VALUE=DATE:{end}\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n",
        uid = uid,
        title = escape_ics_text(title),
        start = format_ics_date(start_date),
        end = format_ics_date(end_date),
    )
}

fn format_ics_date(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

fn escape_ics_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

fn all_day_ics_dates(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timezone: Tz,
) -> (NaiveDate, NaiveDate) {
    let start_date = start.with_timezone(&timezone).date_naive();
    let end_date = end.with_timezone(&timezone).date_naive();
    (start_date, end_date)
}

pub async fn put_calendar_event(
    client: &CalDavClient,
    settings: &Settings,
    request: PutCalendarEventRequest,
) -> Result<PutCalendarEventResponse> {
    let timezone = settings
        .timezone
        .parse::<Tz>()
        .wrap_err_with(|| format!("invalid timezone: {}", settings.timezone))?;

    let ics = if request.all_day {
        let (start_date, end_date) =
            all_day_ics_dates(request.starts_at, request.ends_at, timezone);
        build_all_day_event_ics(&request.uid, &request.title, start_date, end_date)
    } else {
        return Err(eyre!("only all-day events are supported in MVP"));
    };

    let collection = client.calendar_collection_url()?;
    let event_url = event_resource_url(&collection, &request.uid)?;
    let etag = client
        .put_resource(
            &event_url,
            &ics,
            request.etag.as_deref(),
        )
        .await?;

    Ok(PutCalendarEventResponse {
        href: event_url.to_string(),
        etag,
        uid: request.uid,
    })
}

fn event_resource_url(collection: &Url, uid: &str) -> Result<Url> {
    let filename = format!("{uid}.ics");
    collection
        .join(&filename)
        .wrap_err_with(|| format!("failed to build CalDAV event URL for {uid}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_all_day_event_ics_contains_uid_and_dates() {
        let start = NaiveDate::from_ymd_opt(2026, 5, 22).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 5, 23).unwrap();
        let ics = build_all_day_event_ics(
            "nlreminder-todo-abc",
            "TODO: Report",
            start,
            end,
        );

        assert!(ics.contains("UID:nlreminder-todo-abc"));
        assert!(ics.contains("SUMMARY:TODO: Report"));
        assert!(ics.contains("DTSTART;VALUE=DATE:20260522"));
        assert!(ics.contains("DTEND;VALUE=DATE:20260523"));
    }

    #[test]
    fn escape_ics_text_escapes_commas() {
        let start = NaiveDate::from_ymd_opt(2026, 5, 22).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 5, 23).unwrap();
        let ics = build_all_day_event_ics("uid", "A, B", start, end);
        assert!(ics.contains("SUMMARY:A\\, B"));
    }
}
