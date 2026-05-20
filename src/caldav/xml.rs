use color_eyre::eyre::Result;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarCollection {
    pub href: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarObject {
    pub href: Option<String>,
    pub etag: Option<String>,
    pub calendar_data: String,
}

pub fn parse_calendar_collections(xml: &str) -> Result<Vec<CalendarCollection>> {
    let responses = parse_responses(xml)?;
    let mut calendars = Vec::new();

    for response in responses {
        let is_calendar = response
            .resource_types
            .iter()
            .any(|resource_type| resource_type == "calendar");

        if is_calendar {
            calendars.push(CalendarCollection {
                href: response.href,
                display_name: response.display_name,
            });
        }
    }

    Ok(calendars)
}

pub fn parse_calendar_objects(xml: &str) -> Result<Vec<CalendarObject>> {
    let responses = parse_responses(xml)?;
    let mut objects = Vec::new();

    for response in responses {
        if response.calendar_data.is_empty() {
            continue;
        }

        objects.push(CalendarObject {
            href: Some(response.href),
            etag: response.etag,
            calendar_data: response.calendar_data,
        });
    }

    Ok(objects)
}

#[derive(Debug, Default)]
struct ResponseProps {
    href: String,
    display_name: Option<String>,
    resource_types: Vec<String>,
    etag: Option<String>,
    calendar_data: String,
}

#[derive(Debug, Default)]
struct PropStatBuffer {
    display_name: Option<String>,
    resource_types: Vec<String>,
    etag: Option<String>,
    calendar_data: String,
    status_ok: bool,
}

fn parse_responses(xml: &str) -> Result<Vec<ResponseProps>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut responses = Vec::new();
    let mut current: Option<ResponseProps> = None;
    let mut propstat = PropStatBuffer::default();
    let mut current_prop: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) => {
                let name = local_name(&tag);
                match name.as_str() {
                    "response" => {
                        current = Some(ResponseProps::default());
                        propstat = PropStatBuffer::default();
                    }
                    "propstat" => {
                        propstat = PropStatBuffer::default();
                    }
                    "resourcetype" => {
                        propstat.resource_types.clear();
                    }
                    "href" | "displayname" | "getetag" | "calendar-data" | "calendar" | "status" => {
                        current_prop = Some(name);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(tag)) => {
                if local_name(&tag) == "calendar" {
                    propstat.resource_types.push("calendar".to_owned());
                }
            }
            Ok(Event::Text(text)) => {
                let value = text.unescape()?.into_owned();
                match current_prop.as_deref() {
                    Some("href") => {
                        if let Some(response) = current.as_mut() {
                            if response.href.is_empty() {
                                response.href = value;
                            }
                        }
                    }
                    Some("displayname") => propstat.display_name = Some(value),
                    Some("getetag") => propstat.etag = Some(trim_etag(value)),
                    Some("calendar-data") => propstat.calendar_data = value,
                    Some("status") => propstat.status_ok = value.contains("200"),
                    _ => {}
                }
            }
            Ok(Event::End(tag)) => {
                let name = local_name_end(&tag);
                match name.as_str() {
                    "response" => {
                        if let Some(response) = current.take() {
                            if !response.href.is_empty() {
                                responses.push(response);
                            }
                        }
                    }
                    "propstat" => {
                        if propstat.status_ok {
                            if let Some(response) = current.as_mut() {
                                if propstat.display_name.is_some() {
                                    response.display_name = propstat.display_name.clone();
                                }
                                if !propstat.resource_types.is_empty() {
                                    response.resource_types = propstat.resource_types.clone();
                                }
                                if propstat.etag.is_some() {
                                    response.etag = propstat.etag.clone();
                                }
                                if !propstat.calendar_data.is_empty() {
                                    response.calendar_data = propstat.calendar_data.clone();
                                }
                            }
                        }
                        propstat = PropStatBuffer::default();
                    }
                    "href" | "displayname" | "getetag" | "calendar-data" | "status" => {
                        current_prop = None;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err.into()),
        }
    }

    Ok(responses)
}

fn local_name(tag: &quick_xml::events::BytesStart<'_>) -> String {
    let local = tag.name().local_name();
    String::from_utf8_lossy(local.as_ref()).into_owned()
}

fn local_name_end(tag: &quick_xml::events::BytesEnd<'_>) -> String {
    let local = tag.name().local_name();
    String::from_utf8_lossy(local.as_ref()).into_owned()
}

fn trim_etag(value: String) -> String {
    value.trim_matches('"').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPFIND_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/nozyjp/personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    const REPORT_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/nozyjp/personal/event.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
        <C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:abc
SUMMARY:Test
DTSTART:20260520T010000Z
DTEND:20260520T020000Z
END:VEVENT
END:VCALENDAR</C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    #[test]
    fn parse_calendar_collection() {
        let calendars = parse_calendar_collections(PROPFIND_RESPONSE).unwrap();
        assert_eq!(calendars.len(), 1);
        assert_eq!(calendars[0].href, "/nozyjp/personal/");
        assert_eq!(calendars[0].display_name.as_deref(), Some("Personal"));
    }

    #[test]
    fn parse_calendar_object() {
        let objects = parse_calendar_objects(REPORT_RESPONSE).unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].etag.as_deref(), Some("etag-1"));
        assert!(objects[0].calendar_data.contains("BEGIN:VEVENT"));
    }

    #[test]
    fn skips_responses_without_calendar_data() {
        const RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/nozyjp/personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

        let objects = parse_calendar_objects(RESPONSE).unwrap();
        assert!(objects.is_empty());
    }
}
