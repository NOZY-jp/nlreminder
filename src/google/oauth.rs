use color_eyre::eyre::{Context, Result};
use reqwest::Client as HttpClient;
use serde::Deserialize;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GMAIL_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
const TASKS_SCOPE: &str = "https://www.googleapis.com/auth/tasks";

#[derive(Debug, Clone)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub refresh_token_expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TokenResponseBody {
    pub access_token: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub refresh_token_expires_in: Option<u64>,
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/oauth/callback")
}

pub fn authorization_url(client_id: &str, redirect_uri: &str) -> String {
    let scope = format!("{GMAIL_READONLY_SCOPE} {TASKS_SCOPE}");
    format!(
        "{GOOGLE_AUTH_URL}?response_type=code&client_id={client_id}&redirect_uri={redirect_uri}&scope={scope}&access_type=offline&prompt=consent"
    )
}

pub async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<TokenResponseBody> {
    let http = HttpClient::new();
    let response = http
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .wrap_err("failed to call Google token endpoint")?
        .error_for_status()
        .wrap_err("Google token endpoint returned an error")?
        .json::<TokenResponseBody>()
        .await
        .wrap_err("failed to parse Google token response")?;

    Ok(response)
}

pub async fn exchange_refresh_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<AccessTokenResponse> {
    let http = HttpClient::new();
    let response = http
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .wrap_err("failed to call Google token endpoint")?
        .error_for_status()
        .wrap_err("Google token endpoint returned an error")?
        .json::<TokenResponseBody>()
        .await
        .wrap_err("failed to parse Google token response")?;

    Ok(AccessTokenResponse {
        access_token: response.access_token,
        expires_in: response.expires_in,
        refresh_token: response.refresh_token,
        refresh_token_expires_in: response.refresh_token_expires_in,
    })
}

pub async fn refresh_access_token(env: &crate::config::EnvConfig) -> Result<AccessTokenResponse> {
    let refresh_token = env.google_refresh_token()?;
    let (client_id, client_secret) = env.google_oauth_config()?;

    exchange_refresh_token(&client_id, &client_secret, &refresh_token).await
}

pub fn print_token_result(token_response: &TokenResponseBody) -> Result<()> {
    if let Some(refresh_token) = &token_response.refresh_token {
        println!("\nAdd this to .env:\n");
        println!("GOOGLE_REFRESH_TOKEN={refresh_token}");
    } else {
        println!("\nNo refresh token was returned.");
        println!("Revoke nlreminder access in your Google account settings and run setup again.");
    }

    if let Some(expires_in) = token_response.expires_in {
        println!("\nAccess token expires in {expires_in} seconds");
    }

    if let Some(expires_in) = token_response.refresh_token_expires_in {
        println!("\nWarning: refresh_token_expires_in={expires_in} seconds (~7 days).");
        println!("Publish the OAuth consent screen to In production, then run setup again.");
    } else {
        println!("\nNo refresh_token_expires_in field: long-lived refresh token (production mode).");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_uri_uses_localhost() {
        assert_eq!(
            redirect_uri(8080),
            "http://127.0.0.1:8080/oauth/callback"
        );
    }

    #[test]
    fn authorization_url_requests_required_scopes() {
        let url = authorization_url("client-id", "http://127.0.0.1:8080/oauth/callback");
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("gmail.readonly"));
        assert!(url.contains("tasks"));
    }
}
