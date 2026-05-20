mod env;
mod settings;

pub use env::EnvConfig;
pub use settings::Settings;

use std::path::Path;

use color_eyre::eyre::{Context, Result, eyre};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub env: EnvConfig,
    pub settings: Settings,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        let settings = Self::load_settings()?;
        let env = EnvConfig::from_env()?;

        Ok(Self { env, settings })
    }

    fn load_settings() -> Result<Settings> {
        let path = Path::new("config.toml");
        if !path.exists() {
            return Err(eyre!(
                "config.toml not found. Copy config.toml.example to config.toml"
            ));
        }

        let contents = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&contents).wrap_err("failed to parse config.toml")
    }
}
