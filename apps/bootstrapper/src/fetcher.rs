use std::{cmp::min, sync::Arc};

use bindings::{
    i_uniswap_v2erc20::IUniswapV2ERC20, uniswap_v2_factory::UniswapV2Factory,
    uniswap_v2_pair::UniswapV2Pair,
};
use color_eyre::eyre;
use database::DB;
use ethers::{
    abi::Address,
    contract::Multicall,
    providers::{Http, Middleware, Provider},
    types::H160,
};
use futures::future;
use tokio_util::task::TaskTracker;
use tracing::instrument;

/// `Fetcher` fetches from chain data about pairs, tokens and reserves.
#[derive(Clone)]
pub struct Fetcher {
    database: DB,
    eth_client: Arc<Provider<Http>>,
    factory_contract: UniswapV2Factory<Provider<Http>>,
}

pub struct FetcherConfig {
    pub db_url: String,
    pub eth_url: String,
    pub factory_address: Address,
}

impl Fetcher {
    pub async fn try_from_config(config: FetcherConfig) -> eyre::Result<Self> {
        let database = DB::from_url(&config.db_url).await?;
        let eth_client = Arc::new(Provider::<Http>::try_from(config.eth_url.as_str())?);

        Ok(Self::new(database, eth_client, config.factory_address))
    }

    pub fn new(database: DB, eth_client: Arc<Provider<Http>>, factory_address: Address) -> Self {
        let factory_contract = UniswapV2Factory::new(factory_address, eth_client.clone());

        Self {
            database,
            eth_client,
            factory_contract,
        }
    }

    pub async fn run(self) -> eyre::Result<()> {
        let best_block_number = self.eth_client.get_block_number().await?;
        let best_block = self.eth_client.get_block(best_block_number).await?.unwrap();

        let block_id = self.database.insert_block(best_block).await?;
        let factory_id = self
            .database
            .insert_factory(self.factory_contract.address())
            .await?;

        let last_indexed_pair = self.database.last_indexed_pair(factory_id).await? as u64;

        let pairs_length = self
            .factory_contract
            .all_pairs_length()
            .call()
            .await?
            .as_u64();

        tracing::info!(
            "Last indexed pair: {}, pairs length: {}",
            last_indexed_pair,
            pairs_length
        );

        const STEP: usize = 2;
        for pair_num in ((last_indexed_pair + 1)..pairs_length).step_by(STEP) {
            let tracker = TaskTracker::new();

            for pair in pair_num..min(pair_num + STEP as u64, pairs_length) {
                let fetcher = Worker::new(
                    self.database.clone(),
                    self.eth_client.clone(),
                    self.factory_contract.clone(),
                );

                tracker.spawn(async move {
                    // TODO: Instead of skipping the pair, we MUST retry it later
                    if let Err(err) = fetcher.process_pair(factory_id, block_id, pair).await {
                        tracing::error!("Failed to process pair: {:?}", err);
                    }
                });
            }

            tracker.close();
            tracker.wait().await;
        }

        Ok(())
    }
}

/// Worker is responsible for processing a pair and inserting the data into the
/// database.
///
/// Used
pub(crate) struct Worker {
    db: DB,
    eth_client: Arc<Provider<Http>>,
    factory_contract: UniswapV2Factory<Provider<Http>>,
}

impl Worker {
    pub(crate) fn new(
        db: DB,
        eth_client: Arc<Provider<Http>>,
        factory_contract: UniswapV2Factory<Provider<Http>>,
    ) -> Self {
        Self {
            db,
            eth_client,
            factory_contract,
        }
    }

    #[instrument(skip(self, block_id))]
    pub(crate) async fn process_pair(
        &self,
        factory_id: i32,
        block_id: i32,
        pair_num: u64,
    ) -> eyre::Result<()> {
        let pair_address = self
            .factory_contract
            .all_pairs(pair_num.into())
            .call()
            .await?;

        let mut txn = self.db.pool().begin().await?;

        let info = fetch_pair_info(self.eth_client.clone(), pair_address).await?;

        let (token0_id, token1_id) = self
            .fetch_insert_tokens(&mut txn, info.token0, info.token1)
            .await?;

        let pair_id = DB::insert_pair(
            &mut txn,
            pair_address,
            pair_num as i32,
            token0_id,
            token1_id,
            factory_id,
        )
        .await?;

        DB::insert_reserves(&mut txn, pair_id, info.reserve0, info.reserve1, block_id).await?;

        txn.commit().await?;
        tracing::info!("Inserted pair");

        Ok(())
    }

    /// Checks if the tokens exist in the database, if not, fetches the token info
    /// and inserts them into the database.
    ///
    /// This implementation has a optimization to fetch the token info
    /// concurrently if none of the tokens exist in the database.
    // TODO: Refactor this function to be more readable
    async fn fetch_insert_tokens(
        &self,
        txn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        token0: H160,
        token1: H160,
    ) -> Result<(i32, i32), eyre::Error> {
        let (token0_id, token1_id) = DB::check_tokens_exist(txn, token0, token1).await?;

        let (token0_id, token1_id) = match (token0_id, token1_id) {
            // Do nothing if both tokens exist already in the database
            (Some(token0_id), Some(token1_id)) => (token0_id, token1_id),
            (None, Some(token1_id)) => {
                let info = fetch_erc20_info(self.eth_client.clone(), token0).await?;

                let token0_id =
                    DB::insert_token(txn, token0, info.name, info.symbol, info.decimals).await?;

                (token0_id, token1_id)
            }
            (Some(token0_id), None) => {
                let info = fetch_erc20_info(self.eth_client.clone(), token1).await?;

                let token1_id =
                    DB::insert_token(txn, token1, info.name, info.symbol, info.decimals).await?;

                (token0_id, token1_id)
            }
            // If none of the tokens exist, fetch both concurrently and insert them
            (None, None) => {
                let responses = future::join(
                    fetch_erc20_info(self.eth_client.clone(), token0),
                    fetch_erc20_info(self.eth_client.clone(), token1),
                )
                .await;
                let info0 = responses.0?;
                let info1 = responses.1?;

                let token0_id =
                    DB::insert_token(txn, token0, info0.name, info0.symbol, info0.decimals).await?;
                let token1_id =
                    DB::insert_token(txn, token1, info1.name, info1.symbol, info1.decimals).await?;

                (token0_id, token1_id)
            }
        };
        Ok((token0_id, token1_id))
    }
}

#[derive(Debug)]
pub struct TokenInfo {
    /// The name of the token
    pub name: String,

    /// The symbol of the token.
    pub symbol: String,

    /// The number of decimals of the token.
    pub decimals: u8,
}

impl TokenInfo {
    pub fn new(name: String, symbol: String, decimals: u8) -> Self {
        Self {
            name,
            symbol,
            decimals,
        }
    }
}

/// Performs a multicall to fetch the ERC20 token info which includes the
/// name, symbol and decimals
#[instrument(skip(client))]
pub async fn fetch_erc20_info(
    client: Arc<Provider<Http>>,
    address: H160,
) -> Result<TokenInfo, eyre::Error> {
    let token_contract = IUniswapV2ERC20::new(address, client.clone());

    let mut multicall = Multicall::new(client, None).await?;
    multicall
        .add_call(token_contract.name(), false)
        .add_call(token_contract.symbol(), false)
        .add_call(token_contract.decimals(), false);

    let (name, symbol, decimals): (String, String, u8) =
        multicall.call().await.unwrap_or_else(|err| {
            tracing::warn!("Failed to fetch token info: {:?}", err);
            ("unknown".to_string(), "unknown".to_string(), 18)
        });

    Ok(TokenInfo::new(name, symbol, decimals))
}

#[derive(Debug)]
pub struct PairInfo {
    pub token0: Address,
    pub token1: Address,
    pub reserve0: u128,
    pub reserve1: u128,
}

impl PairInfo {
    pub fn new(token0: Address, token1: Address, reserve0: u128, reserve1: u128) -> Self {
        Self {
            token0,
            token1,
            reserve0,
            reserve1,
        }
    }
}

/// Fetches the pair info from the chain using a multicall.
///
/// The pair info includes the token0, token1, reserve0 and reserve1.
pub async fn fetch_pair_info(
    client: Arc<Provider<Http>>,
    address: Address,
) -> Result<PairInfo, eyre::Error> {
    let pair_contract = UniswapV2Pair::new(address, client.clone());

    let mut multicall = Multicall::new(client, None).await?;
    multicall
        .add_call(pair_contract.token_0(), false)
        .add_call(pair_contract.token_1(), false)
        .add_call(pair_contract.get_reserves(), false);

    let (token0, token1, (reserve0, reserve1, _)): (H160, H160, (u128, u128, u32)) =
        multicall.call().await?;

    Ok(PairInfo::new(token0, token1, reserve0, reserve1))
}
