use color_eyre::eyre::{Result, eyre};

#[derive(Debug, Clone)]
pub struct EnvConfig {
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_refresh_token: Option<String>,
    pub google_account_email: String,
    pub lmstudio_model: String,
    pub lmstudio_base_url: String,
    pub caldav_url: String,
    pub caldav_username: String,
    pub caldav_password: String,
    pub discord_token: String,
    pub discord_guild_id: String,
    pub discord_channel_id: String,
    pub oauth_callback_port: u16,
}

#[derive(Copy, Clone)]
pub(crate) enum EnvLoadMode {
    Full,
    GoogleSetup,
}

impl EnvConfig {
    pub fn from_env() -> Result<Self> {
        Self::from_env_inner(EnvLoadMode::Full)
    }

    pub fn from_env_for_google_setup() -> Result<Self> {
        Self::from_env_inner(EnvLoadMode::GoogleSetup)
    }

    fn from_env_inner(mode: EnvLoadMode) -> Result<Self> {
        Ok(Self {
            google_client_id: required_env("GOOGLE_CLIENT_ID")?,
            google_client_secret: required_env("GOOGLE_CLIENT_SECRET")?,
            google_refresh_token: optional_env("GOOGLE_REFRESH_TOKEN"),
            google_account_email: required_env("GOOGLE_ACCOUNT_EMAIL")?,
            lmstudio_model: env_or("LMSTUDIO_MODEL", "Qwen3.6-35B"),
            lmstudio_base_url: env_or("LMSTUDIO_BASE_URL", "http://localhost:1234/v1"),
            caldav_url: optional_or_required("CALDAV_URL", mode)?,
            caldav_username: optional_or_required("CALDAV_USERNAME", mode)?,
            caldav_password: optional_or_required("CALDAV_PASSWORD", mode)?,
            discord_token: optional_or_required("DISCORD_TOKEN", mode)?,
            discord_guild_id: optional_or_required("DISCORD_GUILD_ID", mode)?,
            discord_channel_id: optional_or_required("DISCORD_CHANNEL_ID", mode)?,
            oauth_callback_port: env_or("OAUTH_CALLBACK_PORT", "8080")
                .parse()
                .map_err(|_| color_eyre::eyre::eyre!("OAUTH_CALLBACK_PORT must be a valid u16"))?,
        })
    }

    pub fn google_oauth_config(&self) -> Result<(String, String)> {
        if self.google_client_id.is_empty() || self.google_client_secret.is_empty() {
            return Err(eyre!(
                "GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET must be set in .env"
            ));
        }

        Ok((
            self.google_client_id.clone(),
            self.google_client_secret.clone(),
        ))
    }

    pub fn google_refresh_token(&self) -> Result<String> {
        self.google_refresh_token
            .clone()
            .filter(|token| !token.is_empty())
            .ok_or_else(|| eyre!("GOOGLE_REFRESH_TOKEN is not set. Run: cargo run -- setup google"))
    }
}

fn required_env(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| color_eyre::eyre::eyre!("{key} is not set in .env"))
}

fn optional_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn env_or(key: &str, default: &str) -> String {
    optional_env(key).unwrap_or_else(|| default.to_owned())
}

fn optional_or_required(key: &str, mode: EnvLoadMode) -> Result<String> {
    match mode {
        EnvLoadMode::Full => required_env(key),
        EnvLoadMode::GoogleSetup => Ok(optional_env(key).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn google_setup_vars() -> Vec<(&'static str, Option<&'static str>)> {
        vec![
            ("GOOGLE_CLIENT_ID", Some("client-id")),
            ("GOOGLE_CLIENT_SECRET", Some("client-secret")),
            ("GOOGLE_ACCOUNT_EMAIL", Some("me@example.com")),
            ("GOOGLE_REFRESH_TOKEN", None),
            ("CALDAV_URL", None),
            ("CALDAV_USERNAME", None),
            ("CALDAV_PASSWORD", None),
            ("DISCORD_TOKEN", None),
            ("DISCORD_GUILD_ID", None),
            ("DISCORD_CHANNEL_ID", None),
        ]
    }

    #[test]
    fn google_setup_allows_missing_integration_env() {
        temp_env::with_vars(google_setup_vars(), || {
            let env = EnvConfig::from_env_for_google_setup().expect("google setup env");
            assert_eq!(env.google_client_id, "client-id");
            assert!(env.caldav_url.is_empty());
            assert!(env.discord_token.is_empty());
        });
    }

    #[test]
    fn full_mode_requires_caldav_url() {
        temp_env::with_vars(
            vec![
                ("GOOGLE_CLIENT_ID", Some("client-id")),
                ("GOOGLE_CLIENT_SECRET", Some("client-secret")),
                ("GOOGLE_ACCOUNT_EMAIL", Some("me@example.com")),
                ("CALDAV_URL", None),
                ("CALDAV_USERNAME", Some("user")),
                ("CALDAV_PASSWORD", Some("pass")),
                ("DISCORD_TOKEN", Some("token")),
                ("DISCORD_GUILD_ID", Some("1")),
                ("DISCORD_CHANNEL_ID", Some("2")),
            ],
            || {
                assert!(EnvConfig::from_env().is_err());
            },
        );
    }

    #[test]
    fn lmstudio_defaults_match_requirements() {
        temp_env::with_vars(
            vec![
                ("GOOGLE_CLIENT_ID", Some("client-id")),
                ("GOOGLE_CLIENT_SECRET", Some("client-secret")),
                ("GOOGLE_ACCOUNT_EMAIL", Some("me@example.com")),
                ("CALDAV_URL", Some("https://example.test/")),
                ("CALDAV_USERNAME", Some("user")),
                ("CALDAV_PASSWORD", Some("pass")),
                ("DISCORD_TOKEN", Some("token")),
                ("DISCORD_GUILD_ID", Some("1")),
                ("DISCORD_CHANNEL_ID", Some("2")),
                ("LMSTUDIO_MODEL", None),
                ("LMSTUDIO_BASE_URL", None),
                ("OAUTH_CALLBACK_PORT", None),
            ],
            || {
                let env = EnvConfig::from_env().expect("full env");
                assert_eq!(env.lmstudio_model, "Qwen3.6-35B");
                assert_eq!(env.lmstudio_base_url, "http://localhost:1234/v1");
                assert_eq!(env.oauth_callback_port, 8080);
            },
        );
    }

    #[test]
    fn google_refresh_token_requires_value() {
        let env = EnvConfig {
            google_client_id: "id".to_owned(),
            google_client_secret: "secret".to_owned(),
            google_refresh_token: None,
            google_account_email: "me@example.com".to_owned(),
            lmstudio_model: String::new(),
            lmstudio_base_url: String::new(),
            caldav_url: String::new(),
            caldav_username: String::new(),
            caldav_password: String::new(),
            discord_token: String::new(),
            discord_guild_id: String::new(),
            discord_channel_id: String::new(),
            oauth_callback_port: 8080,
        };

        let err = env.google_refresh_token().unwrap_err();
        assert!(err.to_string().contains("GOOGLE_REFRESH_TOKEN"));
    }
}
