use clap::Parser;
use color_eyre::eyre;

use crate::cli::Cli;

mod cli;
mod actions;

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    cli.run().await
}
