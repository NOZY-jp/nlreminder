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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderTargetType {
    CalendarEvent,
    Todo,
}

impl ReminderTargetType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CalendarEvent => "calendar_event",
            Self::Todo => "todo",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "calendar_event" => Ok(Self::CalendarEvent),
            "todo" => Ok(Self::Todo),
            other => Err(eyre!("invalid reminder target type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderPlanStatus {
    Active,
    Cancelled,
    Completed,
}

impl ReminderPlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "cancelled" => Ok(Self::Cancelled),
            "completed" => Ok(Self::Completed),
            other => Err(eyre!("invalid reminder plan status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderReaction {
    NoResponse,
    Acknowledged,
}

impl ReminderReaction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoResponse => "no_response",
            Self::Acknowledged => "acknowledged",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "no_response" => Ok(Self::NoResponse),
            "acknowledged" => Ok(Self::Acknowledged),
            other => Err(eyre!("invalid reminder reaction: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmCallType {
    MorningSummary,
    SingleReminder,
    MailClassify,
    IgnoreEvaluate,
    PlanCreate,
    DiscordIntent,
}

impl LlmCallType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MorningSummary => "morning_summary",
            Self::SingleReminder => "single_reminder",
            Self::MailClassify => "mail_classify",
            Self::IgnoreEvaluate => "ignore_evaluate",
            Self::PlanCreate => "plan_create",
            Self::DiscordIntent => "discord_intent",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "morning_summary" => Ok(Self::MorningSummary),
            "single_reminder" => Ok(Self::SingleReminder),
            "mail_classify" => Ok(Self::MailClassify),
            "ignore_evaluate" => Ok(Self::IgnoreEvaluate),
            "plan_create" => Ok(Self::PlanCreate),
            "discord_intent" => Ok(Self::DiscordIntent),
            other => Err(eyre!("invalid llm call type: {other}")),
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

    #[test]
    fn roundtrip_reminder_enums() {
        assert_eq!(
            ReminderTargetType::parse("calendar_event").unwrap(),
            ReminderTargetType::CalendarEvent
        );
        assert_eq!(
            ReminderPlanStatus::parse("active").unwrap(),
            ReminderPlanStatus::Active
        );
        assert_eq!(
            ReminderReaction::parse("acknowledged").unwrap(),
            ReminderReaction::Acknowledged
        );
        assert_eq!(
            LlmCallType::parse("plan_create").unwrap(),
            LlmCallType::PlanCreate
        );
    }
}
