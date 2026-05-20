//! External integration tests. Unit tests live next to each module in `src/`.
//!
//! Run ignored tests against a configured environment:
//! `cargo test --test caldav -- --ignored`

use nlreminder::{AppConfig, caldav};

#[tokio::test]
#[ignore = "requires .env and config.toml with CalDAV credentials"]
async fn fetch_week_from_server() {
    dotenvy::dotenv().ok();
    let config = AppConfig::load().expect("config");
    let client = caldav::CalDavClient::new(&config.env, &config.settings).expect("client");

    let (start, end) = caldav::default_fetch_range(&config.settings).expect("range");
    let events = client.fetch_events(start, end).await.expect("fetch");

    assert!(events.iter().all(|event| !event.uid.is_empty()));
}
