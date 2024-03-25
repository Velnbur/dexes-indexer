use clap::Parser;
use color_eyre::eyre;

use crate::cli::Cli;

mod cli;
mod config;
mod database;
pub mod fetcher;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    cli.run().await?;

    Ok(())
}
