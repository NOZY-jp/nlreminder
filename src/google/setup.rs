use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use color_eyre::eyre::{Context, Result, eyre};
use serde::Deserialize;
use tokio::sync::{Mutex, oneshot};
use tokio::time::timeout;
use tracing::info;

use crate::config::EnvConfig;

use super::oauth::{authorization_url, exchange_code, print_token_result, redirect_uri};

pub async fn run_google_setup(env: &EnvConfig) -> Result<()> {
    let (client_id, client_secret) = env.google_oauth_config()?;
    let redirect = redirect_uri(env.oauth_callback_port);
    let authorize_url = authorization_url(&client_id, &redirect);

    let (tx, rx) = oneshot::channel();
    let callback_tx = Arc::new(Mutex::new(Some(tx)));
    let result_slot: Arc<Mutex<Option<Result<String, String>>>> = Arc::new(Mutex::new(None));
    let result_for_handler = Arc::clone(&result_slot);
    let callback_tx_for_handler = Arc::clone(&callback_tx);

    let app = Router::new().route(
        "/oauth/callback",
        get(move |Query(query): Query<CallbackQuery>| {
            let result_for_handler = Arc::clone(&result_for_handler);
            let callback_tx_for_handler = Arc::clone(&callback_tx_for_handler);
            async move {
                let mut slot = result_for_handler.lock().await;
                if slot.is_some() {
                    return Html(
                        "<h1>Already received a callback</h1><p>You can close this tab.</p>"
                            .to_owned(),
                    );
                }

                if let Some(error) = query.error {
                    *slot = Some(Err(error.clone()));
                    if let Some(sender) = callback_tx_for_handler.lock().await.take() {
                        let _ = sender.send(());
                    }
                    return Html(format!(
                        "<h1>Authorization failed</h1><p>{error}</p><p>You can close this tab.</p>"
                    ));
                }

                let Some(code) = query.code else {
                    *slot = Some(Err("missing authorization code".to_owned()));
                    if let Some(sender) = callback_tx_for_handler.lock().await.take() {
                        let _ = sender.send(());
                    }
                    return Html(
                        "<h1>Authorization failed</h1><p>Missing authorization code.</p>".to_owned(),
                    );
                };

                *slot = Some(Ok(code));
                if let Some(sender) = callback_tx_for_handler.lock().await.take() {
                    let _ = sender.send(());
                }
                Html("<h1>Authorization complete</h1><p>You can close this tab and return to the terminal.</p>".to_owned())
            }
        }),
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], env.oauth_callback_port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .wrap_err_with(|| format!("failed to bind OAuth callback server on {addr}"))?;

    info!("OAuth callback server listening on {addr}");

    let server = axum::serve(listener, app);
    let server_handle = tokio::spawn(async move {
        if let Err(err) = server.await {
            tracing::error!("OAuth callback server error: {err}");
        }
    });

    println!("\nOpen this URL in your browser:\n\n{authorize_url}\n");
    println!("Waiting for authorization on {redirect} ...");

    timeout(Duration::from_secs(300), rx)
        .await
        .map_err(|_| eyre!("timed out waiting for OAuth callback (5 minutes)"))?
        .map_err(|_| eyre!("OAuth callback channel closed unexpectedly"))?;

    server_handle.abort();

    let slot = result_slot.lock().await.take();
    let code = match slot {
        Some(Ok(code)) => code,
        Some(Err(err)) => return Err(eyre!("OAuth authorization failed: {err}")),
        None => return Err(eyre!("OAuth callback received no result")),
    };

    let token_response = exchange_code(&client_id, &client_secret, &redirect, &code).await?;
    print_token_result(&token_response)?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    error: Option<String>,
}
