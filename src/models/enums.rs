use color_eyre::eyre::{Result, eyre};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarEventState {
    Scheduled,
    Prepared,
    Completed,
}

impl CalendarEventState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Prepared => "prepared",
            Self::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "scheduled" => Ok(Self::Scheduled),
            "prepared" => Ok(Self::Prepared),
            "completed" => Ok(Self::Completed),
            other => Err(eyre!("invalid calendar event state: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_calendar_event_state() {
        for value in ["scheduled", "prepared", "completed"] {
            let state = CalendarEventState::parse(value).unwrap();
            assert_eq!(state.as_str(), value);
        }
    }
}
