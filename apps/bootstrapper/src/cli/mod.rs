use std::{path::PathBuf, str::FromStr};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{self, Context};
use tracing::level_filters::LevelFilter;

use config::Config;

use self::actions::run::RunArgs;

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
            Commands::Run(args) => {
                actions::run::run(config, args).await?;
            }
        }

        Ok(())
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the full node.
    Run(RunArgs),
}
