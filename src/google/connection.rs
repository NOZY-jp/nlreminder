use color_eyre::eyre::{Context, Result, eyre};
use reqwest::Client as HttpClient;
use serde::Deserialize;

use crate::config::EnvConfig;

use super::oauth::refresh_access_token;

pub async fn check_google_connection(env: &EnvConfig) -> Result<()> {
    let token = refresh_access_token(env).await?;

    if let Some(expires_in) = token.expires_in {
        println!("Access token expires in {expires_in} seconds");
    }

    if let Some(expires_in) = token.refresh_token_expires_in {
        println!(
            "Warning: refresh_token_expires_in={expires_in} (~7 days). Publish OAuth app to In production."
        );
    }

    let http = HttpClient::new();
    let profile: GmailProfile = http
        .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
        .bearer_auth(&token.access_token)
        .send()
        .await
        .wrap_err("failed to call Gmail profile API")?
        .error_for_status()
        .wrap_err("Gmail profile API returned an error")?
        .json()
        .await
        .wrap_err("failed to parse Gmail profile response")?;

    println!("Gmail connection OK");
    println!("  emailAddress: {}", profile.email_address);
    println!("  expected:     {}", env.google_account_email);

    if profile.email_address != env.google_account_email {
        return Err(eyre!(
            "Gmail profile email does not match GOOGLE_ACCOUNT_EMAIL"
        ));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct GmailProfile {
    #[serde(rename = "emailAddress")]
    email_address: String,
}
