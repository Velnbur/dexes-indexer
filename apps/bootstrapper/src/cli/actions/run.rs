use color_eyre::eyre;
use ethers::types::Address;

use crate::fetcher::{Fetcher, FetcherConfig};
use config::Config;

pub async fn run(config: Config, factory_address: Address) -> eyre::Result<()> {
    let fetcher = Fetcher::try_from_config(FetcherConfig {
        db_url: config.database.url,
        eth_url: config.ethereum.url,
        factory_address,
    })
    .await?;

    fetcher.run().await?;

    Ok(())
}
