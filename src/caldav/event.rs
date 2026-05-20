use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use color_eyre::eyre::{Context, Result, eyre};
use icalendar::parser::{Component as ParsedComponent, Property, read_calendar};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalDavEvent {
    pub uid: String,
    pub etag: Option<String>,
    pub href: Option<String>,
    pub summary: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
}

pub fn parse_calendar_data(
    calendar_data: &str,
    etag: Option<String>,
    href: Option<String>,
    default_timezone: Tz,
) -> Result<Vec<CalDavEvent>> {
    let calendar = read_calendar(calendar_data).map_err(|err| eyre!("failed to parse calendar-data: {err}"))?;
    let mut events = Vec::new();
    collect_events(
        &calendar.components,
        &mut events,
        etag.clone(),
        href.clone(),
        default_timezone,
    )?;
    Ok(events)
}

fn collect_events(
    components: &[ParsedComponent<'_>],
    events: &mut Vec<CalDavEvent>,
    etag: Option<String>,
    href: Option<String>,
    default_timezone: Tz,
) -> Result<()> {
    for component in components {
        if component.name.as_ref() == "VEVENT" {
            events.push(parse_vevent(
                component,
                etag.clone(),
                href.clone(),
                default_timezone,
            )?);
        }
        collect_events(
            &component.components,
            events,
            etag.clone(),
            href.clone(),
            default_timezone,
        )?;
    }
    Ok(())
}

fn parse_vevent(
    component: &ParsedComponent<'_>,
    etag: Option<String>,
    href: Option<String>,
    default_timezone: Tz,
) -> Result<CalDavEvent> {
    let uid = property_value(component, "UID")
        .ok_or_else(|| eyre!("VEVENT missing UID"))?
        .to_owned();

    let summary = property_value(component, "SUMMARY").unwrap_or("(no title)").to_owned();

    let (starts_at, all_day_start) = component
        .find_prop("DTSTART")
        .map(|property| parse_property_datetime(property, default_timezone))
        .transpose()?
        .unwrap_or((None, false));

    let (ends_at, all_day_end) = component
        .find_prop("DTEND")
        .map(|property| parse_property_datetime(property, default_timezone))
        .transpose()?
        .unwrap_or((None, false));

    Ok(CalDavEvent {
        uid,
        etag,
        href,
        summary,
        starts_at,
        ends_at,
        all_day: all_day_start || all_day_end,
    })
}

fn property_value<'a>(component: &'a ParsedComponent<'a>, key: &str) -> Option<&'a str> {
    component.find_prop(key).map(|property| property.val.as_ref())
}

fn parse_property_datetime(property: &Property<'_>, default_timezone: Tz) -> Result<(Option<DateTime<Utc>>, bool)> {
    let tzid = property
        .params
        .iter()
        .find(|param| param.key.as_ref() == "TZID")
        .and_then(|param| param.val.as_ref().map(|value| value.as_ref()));
    parse_ics_datetime(property.val.as_ref(), tzid, default_timezone)
}

fn parse_ics_datetime(
    value: &str,
    tzid: Option<&str>,
    default_timezone: Tz,
) -> Result<(Option<DateTime<Utc>>, bool)> {
    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Ok((Some(datetime.with_timezone(&Utc)), false));
    }

    if value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")
            .wrap_err_with(|| format!("invalid all-day date: {value}"))?;
        let datetime = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| eyre!("invalid date components"))?
            .and_utc();
        return Ok((Some(datetime), true));
    }

    if value.ends_with('Z') && value.len() == 16 {
        let date = NaiveDate::parse_from_str(&value[..8], "%Y%m%d")
            .wrap_err_with(|| format!("invalid UTC datetime date: {value}"))?;
        let time = chrono::NaiveTime::parse_from_str(&value[9..15], "%H%M%S")
            .wrap_err_with(|| format!("invalid UTC datetime time: {value}"))?;
        return Ok((Some(date.and_time(time).and_utc()), false));
    }

    if value.len() == 15 {
        let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S")
            .wrap_err_with(|| format!("invalid floating datetime: {value}"))?;
        let timezone = tzid
            .map(str::parse::<Tz>)
            .transpose()
            .wrap_err_with(|| format!("invalid TZID: {tzid:?}"))?
            .unwrap_or(default_timezone);
        let datetime = timezone
            .from_local_datetime(&naive)
            .single()
            .ok_or_else(|| eyre!("ambiguous local datetime: {value}"))?
            .with_timezone(&Utc);
        return Ok((Some(datetime), false));
    }

    Err(eyre!("unsupported ICS datetime format: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_ICS: &str = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:test-event-1
SUMMARY:Team meeting
DTSTART:20260520T010000Z
DTEND:20260520T020000Z
END:VEVENT
END:VCALENDAR
"#;

    #[test]
    fn parse_sample_event() {
        let events = parse_calendar_data(SAMPLE_ICS, Some("etag-1".to_owned()), None, chrono_tz::Asia::Tokyo).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "test-event-1");
        assert_eq!(events[0].summary, "Team meeting");
        assert_eq!(events[0].etag.as_deref(), Some("etag-1"));
        assert!(!events[0].all_day);
        assert_eq!(
            events[0].starts_at.unwrap().to_rfc3339(),
            "2026-05-20T01:00:00+00:00"
        );
    }

    #[test]
    fn parse_all_day_event() {
        const ICS: &str = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:all-day-1
SUMMARY:Holiday
DTSTART:20260521
DTEND:20260522
END:VEVENT
END:VCALENDAR
"#;

        let events = parse_calendar_data(ICS, None, None, chrono_tz::Asia::Tokyo).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].all_day);
        assert_eq!(events[0].uid, "all-day-1");
    }

    #[test]
    fn parse_floating_datetime_with_default_timezone() {
        const ICS: &str = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:floating-1
SUMMARY:Lunch
DTSTART:20260521T120000
DTEND:20260521T130000
END:VEVENT
END:VCALENDAR
"#;

        let events = parse_calendar_data(ICS, None, None, chrono_tz::Asia::Tokyo).unwrap();
        assert_eq!(events.len(), 1);
        assert!(!events[0].all_day);
        // 2026-05-21 12:00 JST = 03:00 UTC
        assert_eq!(
            events[0].starts_at.unwrap().to_rfc3339(),
            "2026-05-21T03:00:00+00:00"
        );
    }

    #[test]
    fn parse_tzid_datetime() {
        const ICS: &str = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:tzid-1
SUMMARY:With TZID
DTSTART;TZID=Asia/Tokyo:20260521T120000
END:VEVENT
END:VCALENDAR
"#;

        let events = parse_calendar_data(ICS, None, None, chrono_tz::UTC).unwrap();
        assert_eq!(
            events[0].starts_at.unwrap().to_rfc3339(),
            "2026-05-21T03:00:00+00:00"
        );
    }

    #[test]
    fn missing_uid_is_error() {
        const ICS: &str = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
SUMMARY:No UID
DTSTART:20260521T120000
END:VEVENT
END:VCALENDAR
"#;

        let err = parse_calendar_data(ICS, None, None, chrono_tz::Asia::Tokyo).unwrap_err();
        assert!(err.to_string().contains("UID"));
    }

    #[test]
    fn missing_summary_uses_placeholder() {
        const ICS: &str = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:no-summary
DTSTART:20260521T120000Z
END:VEVENT
END:VCALENDAR
"#;

        let events = parse_calendar_data(ICS, None, None, chrono_tz::Asia::Tokyo).unwrap();
        assert_eq!(events[0].summary, "(no title)");
    }
}
