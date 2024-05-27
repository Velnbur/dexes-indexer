use std::path::PathBuf;

use config::Config;
use clap::{Args, Parser, Subcommand};
use color_eyre::eyre;
use ethers::types::Address;

use crate::actions;

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,

    #[clap(short = 'v', action = clap::ArgAction::Count)]
    pub verbosity: u32,

    #[clap(short = 'c', long)]
    pub config: PathBuf,
}

impl Cli {
    pub async fn run(self) -> eyre::Result<()> {
        let config = Config::load(&self.config)?;

        match self.command {
            Commands::Find(args) => actions::find(config, args).await?,
        }

        Ok(())
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Find(FindArgs),
}

#[derive(Debug, Args)]
pub struct FindArgs {
    #[clap(short, long)]
    pub from: Address,

    #[clap(short, long)]
    pub to: Address,

    #[clap(short, long)]
    pub value: u64,
}
