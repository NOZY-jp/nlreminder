use std::time::Duration;

use color_eyre::eyre::Result;
use tokio::time::MissedTickBehavior;

use crate::{AppConfig, caldav, db, sync};

pub async fn run() -> Result<()> {
    let config = AppConfig::load()?;
    let pool = db::connect_and_migrate(&config.settings.database_path).await?;
    let client = caldav::CalDavClient::new(&config.env, &config.settings)?;

    tracing::info!("nlreminder daemon started");
    tracing::info!("database: {}", config.settings.database_path.display());
    tracing::info!("timezone: {}", config.settings.timezone);
    tracing::info!("scan interval: {}s", config.settings.scan_interval_secs);

    let mut scan_interval =
        tokio::time::interval(Duration::from_secs(config.settings.scan_interval_secs));
    scan_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = scan_interval.tick() => {
                match sync::sync_caldav_to_db(&pool, &client, &config.settings).await {
                    Ok(report) => {
                        tracing::info!(
                            upserted = report.upserted,
                            completed = report.completed,
                            "caldav sync finished"
                        );
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "caldav sync failed");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutting down");
                break;
            }
        }
    }

    Ok(())
}
