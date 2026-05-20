mod cli;

use color_eyre::eyre::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("nlreminder=info".parse()?))
        .init();

    cli::run().await
}
