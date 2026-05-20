mod connection;
mod gmail;
mod oauth;
mod setup;
mod tasks;

pub use connection::check_google_connection;
pub use gmail::{
    GmailMessage, HistoryFetchResult, INITIAL_LOOKBACK_DAYS, dedupe_message_ids,
    fetch_profile_history_id, get_message, gmail_after_query, initial_sync_after_date,
    list_history_message_ids, list_message_ids_for_query, parse_history_list_response,
    parse_message_response, parse_messages_list_response, parse_profile_response,
};
pub use oauth::{AccessTokenResponse, refresh_access_token};
pub use setup::run_google_setup;
pub use tasks::{
    GoogleTask, all_day_range_for_due, list_tasks, parse_tasks_list_response, resolve_list_id,
};
