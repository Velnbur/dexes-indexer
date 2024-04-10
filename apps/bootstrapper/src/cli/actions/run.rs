use clap::Args;
use color_eyre::eyre;
use ethers::types::Address;
use tokio_util::sync::CancellationToken;

use crate::fetcher::{IndexerConfig, IndexerPool};
use config::Config;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The address of the factory contract.
    #[arg(short, long)]
    pub factory_address: Address,

    /// URL to the Ethereum node.
    #[arg(short = 'e', long = "eth-url")]
    pub ethereum_provider_url: String,

    /// The number of workers to spawn.
    #[arg(short, long, default_value = "1")]
    pub workers: u32,
}

pub async fn run(
    config: Config,
    RunArgs {
        factory_address,
        ethereum_provider_url,
        workers,
    }: RunArgs,
) -> eyre::Result<()> {
    let ctrl_c = tokio::signal::ctrl_c();
    let cancellation = CancellationToken::new();

    let indexer = IndexerPool::try_from_config(
        IndexerConfig {
            db_url: config.database.url,
            eth_url: ethereum_provider_url,
            factory_address,
            // TODO: make this configurable
            concurrency: workers as usize,
        },
        cancellation.clone(),
    )
    .await?;

    tokio::select! {
        _ = ctrl_c => cancellation.cancel(),
        res = indexer.run() => {
            res?;
        }
    }

    Ok(())
}
