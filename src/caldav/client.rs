use std::time::Duration;

use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result, eyre};
use reqwest::Client;
use url::Url;

use crate::config::{EnvConfig, Settings};

use super::event::CalDavEvent;
use super::xml::CalendarCollection;

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>
"#;

pub struct CalDavClient {
    http: Client,
    base_url: Url,
    username: String,
    password: String,
    calendar_path: String,
    timezone: String,
}

impl CalDavClient {
    pub fn new(env: &EnvConfig, settings: &Settings) -> Result<Self> {
        let base_url = normalize_base_url(&env.caldav_url)?;

        Ok(Self {
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .wrap_err("failed to build HTTP client")?,
            base_url,
            username: env.caldav_username.clone(),
            password: env.caldav_password.clone(),
            calendar_path: settings.caldav_calendar_path.clone(),
            timezone: settings.timezone.clone(),
        })
    }

    fn timezone(&self) -> Result<chrono_tz::Tz> {
        self.timezone
            .parse()
            .wrap_err_with(|| format!("invalid timezone: {}", self.timezone))
    }

    pub async fn list_calendars(&self) -> Result<Vec<CalendarCollection>> {
        let target = self.discovery_url()?;
        let xml = self.propfind(&target, "1").await?;
        let mut calendars = super::xml::parse_calendar_collections(&xml)?;

        if calendars.is_empty() {
            let root_xml = self.propfind(&self.base_url, "0").await?;
            let root_calendars = super::xml::parse_calendar_collections(&root_xml)?;
            calendars.extend(root_calendars);
        }

        Ok(calendars)
    }

    pub async fn fetch_events(
        &self,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
    ) -> Result<Vec<CalDavEvent>> {
        let calendar_urls = self.resolve_calendar_urls().await?;
        let mut all_events = Vec::new();

        for calendar_url in calendar_urls {
            let xml = self
                .calendar_query(&calendar_url, range_start, range_end)
                .await?;
            let objects = super::xml::parse_calendar_objects(&xml)?;

            for object in objects {
                let mut events = super::event::parse_calendar_data(
                    &object.calendar_data,
                    object.etag.clone(),
                    object.href.clone(),
                    self.timezone()?,
                )?;
                all_events.append(&mut events);
            }
        }

        all_events.sort_by(|left, right| left.starts_at.cmp(&right.starts_at));
        Ok(all_events)
    }

    pub fn calendar_collection_url(&self) -> Result<Url> {
        if !self.calendar_path.is_empty() {
            join_url(&self.base_url, &self.calendar_path)
        } else {
            Ok(self.base_url.clone())
        }
    }

    pub async fn put_resource(
        &self,
        url: &Url,
        body: &str,
        if_match: Option<&str>,
    ) -> Result<String> {
        let mut request = self
            .http
            .request(reqwest::Method::PUT, url.clone())
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "text/calendar; charset=utf-8")
            .body(body.to_owned());

        if let Some(etag) = if_match {
            request = request.header("If-Match", etag);
        }

        let response = request
            .send()
            .await
            .wrap_err_with(|| format!("CalDAV PUT failed for {}", url))?
            .error_for_status()
            .wrap_err_with(|| format!("CalDAV server returned an error for PUT {}", url))?;

        let etag = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim_matches('"').to_owned())
            .unwrap_or_else(|| format!("\"{}\"", url));

        Ok(etag)
    }

    async fn resolve_calendar_urls(&self) -> Result<Vec<Url>> {
        if !self.calendar_path.is_empty() {
            return Ok(vec![join_url(&self.base_url, &self.calendar_path)?]);
        }

        let calendars = self.list_calendars().await?;
        if calendars.is_empty() {
            return Ok(vec![self.base_url.clone()]);
        }

        calendars
            .into_iter()
            .map(|calendar| join_url(&self.base_url, &calendar.href))
            .collect()
    }

    fn discovery_url(&self) -> Result<Url> {
        if self.calendar_path.is_empty() {
            Ok(self.base_url.clone())
        } else {
            join_url(&self.base_url, &self.calendar_path)
        }
    }

    async fn propfind(&self, url: &Url, depth: &str) -> Result<String> {
        self.dav_request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), url, depth, PROPFIND_BODY)
            .await
    }

    async fn calendar_query(
        &self,
        url: &Url,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
    ) -> Result<String> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag/>
    <C:calendar-data/>
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VEVENT">
        <C:time-range start="{}" end="{}"/>
      </C:comp-filter>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>
"#,
            format_caldav_time(range_start),
            format_caldav_time(range_end)
        );

        self.dav_request(reqwest::Method::from_bytes(b"REPORT").unwrap(), url, "1", &body)
            .await
    }

    async fn dav_request(
        &self,
        method: reqwest::Method,
        url: &Url,
        depth: &str,
        body: &str,
    ) -> Result<String> {
        let response = self
            .http
            .request(method, url.clone())
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body.to_owned())
            .send()
            .await
            .wrap_err_with(|| format!("CalDAV request failed for {}", url))?
            .error_for_status()
            .wrap_err_with(|| format!("CalDAV server returned an error for {}", url))?
            .text()
            .await
            .wrap_err("failed to read CalDAV response body")?;

        Ok(response)
    }
}

fn normalize_base_url(raw: &str) -> Result<Url> {
    let mut url = Url::parse(raw).wrap_err_with(|| format!("invalid CALDAV_URL: {raw}"))?;
    if !url.path().ends_with('/') {
        url.path_segments_mut()
            .map_err(|_| eyre!("CALDAV_URL cannot be a base URL"))?
            .push("");
    }
    Ok(url)
}

fn join_url(base: &Url, path: &str) -> Result<Url> {
    if let Ok(absolute) = Url::parse(path) {
        return Ok(absolute);
    }

    if path.starts_with('/') {
        let mut url = base.clone();
        url.set_path(path);
        url.set_query(None);
        url.set_fragment(None);
        return Ok(url);
    }

    base.join(path)
        .wrap_err_with(|| format!("failed to join CalDAV path: {path}"))
}

fn format_caldav_time(value: DateTime<Utc>) -> String {
    value.format("%Y%m%dT%H%M%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn format_caldav_time_uses_utc() {
        let value = DateTime::parse_from_rfc3339("2026-05-20T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(format_caldav_time(value), "20260520T010000Z");
    }

    #[test]
    fn normalize_base_url_adds_trailing_slash() {
        let url = normalize_base_url("https://example.test/nozyjp").unwrap();
        assert_eq!(url.as_str(), "https://example.test/nozyjp/");
    }

    #[test]
    fn join_url_supports_absolute_path() {
        let base = Url::parse("https://example.test/nozyjp/").unwrap();
        let joined = join_url(&base, "/nozyjp/sshCalendar/").unwrap();
        assert_eq!(joined.as_str(), "https://example.test/nozyjp/sshCalendar/");
    }
}
