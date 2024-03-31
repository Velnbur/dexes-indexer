use color_eyre::eyre;
use ethers::types::Address;
use tokio_util::sync::CancellationToken;

use crate::fetcher::{IndexerConfig, IndexerPool};
use config::Config;

pub async fn run(
    config: Config,
    factory_address: Address,
    workers: usize,
) -> eyre::Result<()> {
    let ctrl_c = tokio::signal::ctrl_c();
    let cancellation = CancellationToken::new();

    let indexer = IndexerPool::try_from_config(
        IndexerConfig {
            db_url: config.database.url,
            eth_url: config.ethereum.url,
            factory_address,
            // TODO: make this configurable
            concurrency: workers,
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
