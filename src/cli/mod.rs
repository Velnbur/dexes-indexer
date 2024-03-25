use std::{path::PathBuf, str::FromStr};

use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::{self, Context};
use ethers::abi::Address;
use tracing::level_filters::LevelFilter;

use crate::config::Config;

mod actions;

#[derive(Parser, Debug)]
#[command(version, about, author)]
pub struct Cli {
    #[arg(short, long)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub async fn run(self) -> eyre::Result<()> {
        let config = Config::load(self.config)?;

        let level_filter = LevelFilter::from_str(config.logger.level.as_str())
            .wrap_err("Invalid logger level specified")?;

        let log_path = config.logger.path.clone();
        tracing_subscriber::fmt()
            .with_writer(move || -> Box<dyn std::io::Write> {
                if let Some(log_file) = &log_path {
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(log_file)
                        .expect("Failed to open log file");

                    Box::new(file)
                } else {
                    Box::new(std::io::stdout())
                }
            })
            .with_max_level(level_filter)
            .init();

        match self.command {
            Commands::Run => {
                unimplemented!()
            }
            Commands::Bootstrap(BootstrapArgs { factory_address }) => {
                actions::bootstrap::run(config, factory_address).await?;
            }
        }

        Ok(())
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the full node.
    Run,

    /// Bootstrap the local database with the latest data from the Ethereum
    /// node.
    Bootstrap(BootstrapArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {}

#[derive(Args, Debug)]
pub struct BootstrapArgs {
    #[arg(short, long)]
    pub factory_address: Address,
}
