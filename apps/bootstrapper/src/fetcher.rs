use std::sync::Arc;

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
use sqlx::PgConnection;
use tokio::select;
use tokio_util::{task::TaskTracker, sync::CancellationToken};
use tracing::instrument;

/// Indexer fetches from chain data about pairs, tokens and reserves.
pub struct IndexerPool {
    /// Database connector to store indexing results
    database: DB,

    /// Client to interact with the Ethereum network
    eth_client: Arc<Provider<Http>>,

    /// The factory contract provider
    factory_contract: UniswapV2Factory<Provider<Http>>,

    /// Cancellation token to stop the indexer
    cancellation: CancellationToken,

    /// Task tracker of the workers
    tracker: TaskTracker,

    /// Sender of the pair nums to the workers
    tx: flume::Sender<Task>,
}

pub struct IndexerConfig {
    pub db_url: String,
    pub eth_url: String,
    pub factory_address: Address,

    /// Number of concurrent workers to process pairs.
    pub concurrency: usize,
}

impl IndexerPool {
    pub async fn try_from_config(config: IndexerConfig, cancellation: CancellationToken) -> eyre::Result<Self> {
        let database = DB::from_url(&config.db_url).await?;
        let eth_client = Arc::new(Provider::<Http>::try_from(config.eth_url.as_str())?);

        Ok(Self::new(database, eth_client, config, cancellation))
    }

    pub fn new(database: DB, eth_client: Arc<Provider<Http>>, config: IndexerConfig, cancellation: CancellationToken) -> Self {
        let factory_contract = UniswapV2Factory::new(config.factory_address, eth_client.clone());
        let (tx, rx) = flume::bounded(config.concurrency);
        let tracker = TaskTracker::new();

        for _i in 0..config.concurrency {
            let worker = Worker::new(
                database.clone(),
                eth_client.clone(),
                factory_contract.clone(),
                cancellation.child_token(),
                rx.clone(),
            );
            tracker.spawn(async move {
                if let Err(err) = worker.run().await {
                    tracing::error!(?err, "Worker failed");
                }
            });
        }
        tracker.close();

        Self {
            database,
            eth_client,
            factory_contract,
            cancellation,
            tracker,
            tx,
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

        for pair_num in (last_indexed_pair + 1)..pairs_length {
            select! {
                _ = self.cancellation.cancelled() => break,
                _ = self.tx.send_async(Task {
                    factory_id,
                    block_id,
                    pair_num,
                }) => {}
            }
        }

        self.tracker.wait().await;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Task {
    pub factory_id: i32,
    pub block_id: i32,
    pub pair_num: u64,
}

/// Worker is responsible for processing a pair and inserting the data into the
/// database.
///
/// Used
pub(crate) struct Worker {
    db: DB,
    eth_client: Arc<Provider<Http>>,
    factory_contract: UniswapV2Factory<Provider<Http>>,
    cancellation: CancellationToken,
    rx: flume::Receiver<Task>,
}

impl Worker {
    pub(crate) fn new(
        db: DB,
        eth_client: Arc<Provider<Http>>,
        factory_contract: UniswapV2Factory<Provider<Http>>,
        cancellation: CancellationToken,
        rx: flume::Receiver<Task>,
    ) -> Self {
        Self {
            db,
            eth_client,
            factory_contract,
            cancellation,
            rx,
        }
    }

    #[instrument(skip(self), name = "Worker")]
    pub(crate) async fn run(&self) -> eyre::Result<()> {
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
        const RETRIES: usize = 3;

        loop {
            let task = select! {
                msg = self.rx.recv_async() => msg? ,
                _ = self.cancellation.cancelled() => break,
            };

            for _ in 0..RETRIES {
                if let Err(err) = self.process_pair(task).await {
                    tracing::warn!("Failed to process pair: {:?}", err);
                    tokio::time::sleep(RETRY_DELAY).await;
                } else {
                    break;
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn process_pair(
        &self,
        Task {
            factory_id,
            block_id,
            pair_num,
        }: Task,
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
        txn: &mut PgConnection,
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
