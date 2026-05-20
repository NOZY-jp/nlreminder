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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoState {
    Todo,
    Ongoing,
    Done,
}

impl TodoState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Ongoing => "ongoing",
            Self::Done => "done",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "todo" => Ok(Self::Todo),
            "ongoing" => Ok(Self::Ongoing),
            "done" => Ok(Self::Done),
            other => Err(eyre!("invalid todo state: {other}")),
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

    #[test]
    fn roundtrip_todo_state() {
        for value in ["todo", "ongoing", "done"] {
            let state = TodoState::parse(value).unwrap();
            assert_eq!(state.as_str(), value);
        }
    }
}
