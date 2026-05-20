//! Integration test against the real Gmail API.
//!
//! Run locally with credentials configured:
//! `cargo test --test gmail fetch_from_server -- --ignored --nocapture`

use nlreminder::config::AppConfig;
use nlreminder::db;
use nlreminder::sync::fetch_new_gmail_messages;

#[tokio::test]
#[ignore = "requires .env with Google OAuth credentials"]
async fn fetch_from_server() {
    dotenvy::dotenv().ok();
    let config = AppConfig::load().expect("config");
    let pool = db::connect_and_migrate(&config.settings.database_path)
        .await
        .expect("db");

    let messages = fetch_new_gmail_messages(&pool, &config.env)
        .await
        .expect("gmail sync");

    eprintln!("fetched {} gmail messages", messages.len());
    for message in messages.iter().take(3) {
        eprintln!(
            "- {} | {:?} | {:?}",
            message.message_id, message.subject, message.sender
        );
    }
}
