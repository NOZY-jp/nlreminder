use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;

use nlreminder::{EnvConfig, app, google};

#[derive(Debug, Parser)]
#[command(name = "nlreminder")]
#[command(about = "LLM-powered reminder system")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run the nlreminder daemon
    Run,
    /// One-time setup (human-operated; not used by LLM/MCP)
    Setup {
        #[command(subcommand)]
        command: SetupCommands,
    },
}

#[derive(Debug, Subcommand)]
enum SetupCommands {
    /// Run Google OAuth flow and obtain a refresh token
    Google {
        /// Verify an existing refresh token instead of running OAuth
        #[arg(long)]
        check: bool,
    },
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run => app::run().await,
        Commands::Setup { command } => match command {
            SetupCommands::Google { check } => {
                dotenvy::dotenv().ok();
                let env = EnvConfig::from_env_for_google_setup()?;
                if check {
                    google::check_google_connection(&env).await
                } else {
                    google::run_google_setup(&env).await
                }
            }
        },
    }
}
