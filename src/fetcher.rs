use std::{sync::Arc, time::Duration};

use bindings::{
    i_uniswap_v2erc20::IUniswapV2ERC20, uniswap_v2_factory::UniswapV2Factory,
    uniswap_v2_pair::UniswapV2Pair,
};
use color_eyre::eyre;
use ethers::{
    abi::Address,
    providers::{Http, Middleware, Provider},
    types::{Block, H160, H256},
};
use sqlx::PgConnection;
use tokio::time::sleep;

/// `Fetcher` fetches from chain data about pairs, tokens and reserves.
pub struct Fetcher {
    pool: sqlx::PgPool,
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
        let pool = sqlx::PgPool::connect(&config.db_url).await?;
        let eth_client = Arc::new(Provider::<Http>::try_from(config.eth_url.as_str())?);

        Ok(Self::new(pool, eth_client, config.factory_address))
    }

    pub fn new(
        pool: sqlx::PgPool,
        eth_client: Arc<Provider<Http>>,
        factory_address: Address,
    ) -> Self {
        let factory_contract = UniswapV2Factory::new(factory_address, eth_client.clone());

        Self {
            pool,
            eth_client,
            factory_contract,
        }
    }

    pub async fn run(self) -> eyre::Result<()> {
        let best_block_number = self.eth_client.get_block_number().await?;
        let best_block = self.eth_client.get_block(best_block_number).await?.unwrap();

        let block_id = self.insert_block(best_block).await?;

        let factory_id = self.insert_factory(self.factory_contract.address()).await?;

        let last_indexed_pair = self.last_indexed_pair(factory_id).await? + 1;

        let pairs_length = self
            .factory_contract
            .all_pairs_length()
            .call()
            .await?
            .as_u64();

        for pair_num in (last_indexed_pair as u64)..pairs_length {
            let span = tracing::info_span!("fetching_pair", ?pair_num);
            let _guard = span.enter();

            let pair_address = self
                .factory_contract
                .all_pairs(pair_num.into())
                .call()
                .await?;

            let mut txn = self.pool.begin().await?;

            let pair_contract = UniswapV2Pair::new(pair_address, self.eth_client.clone());

            let pair_token0_call = pair_contract.token_0();
            let pair_token1_call = pair_contract.token_1();
            let (pair_token0, pair_token1) =
                futures::join!(pair_token0_call.call(), pair_token1_call.call());
            let pair_token0 = pair_token0?;
            let pair_token1 = pair_token1?;

            let token0_id = self.fetch_and_insert_token(&mut txn, pair_token0).await?;
            let token1_id = self.fetch_and_insert_token(&mut txn, pair_token1).await?;

            let pair_id = self
                .insert_pair(
                    &mut txn,
                    pair_address,
                    pair_num as i32,
                    token0_id,
                    token1_id,
                    factory_id,
                )
                .await?;

            let (reserve0, reserve1, _) = pair_contract.get_reserves().call().await?;

            self.insert_reserves(&mut txn, pair_id, reserve0, reserve1, block_id)
                .await?;

            txn.commit().await?;
            sleep(Duration::from_millis(500)).await;
            tracing::info!("Inserted pair");
        }

        Ok(())
    }

    async fn insert_factory(&self, address: Address) -> eyre::Result<i32> {
        let mut txn = self.pool.begin().await?;

        let factory_exists = sqlx::query!(
            "SELECT id FROM factories WHERE address = $1",
            address.to_string()
        )
        .fetch_optional(&mut *txn)
        .await?;

        if let Some(factory_id) = factory_exists {
            return Ok(factory_id.id);
        }

        let factory_id = sqlx::query!(
            r#"
            INSERT INTO factories (address)
            VALUES ($1)
            RETURNING id
            "#,
            address.to_string(),
        )
        .fetch_one(&mut *txn)
        .await?;

        txn.commit().await?;

        Ok(factory_id.id)
    }

    async fn last_indexed_pair(&self, factory_id: i32) -> eyre::Result<i32> {
        let last_indexed_pair = sqlx::query!(
            "SELECT MAX(number) FROM pairs WHERE factory = $1",
            factory_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(last_indexed_pair.max.unwrap_or(0))
    }

    async fn insert_block(&self, block: Block<H256>) -> eyre::Result<i32> {
        let block_number = block.number.unwrap().as_u64() as i64;
        let block_hash = block.hash.unwrap();

        let id_entry = sqlx::query!(
            r#"
            INSERT INTO blocks (height, hash)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            RETURNING id
            "#,
            block_number,
            block_hash.to_string()
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(id_entry.id)
    }

    pub async fn fetch_and_insert_token(
        &self,
        conn: &mut PgConnection,
        address: H160,
    ) -> eyre::Result<i32> {
        let token_exists = sqlx::query!(
            "SELECT id FROM tokens WHERE address = $1",
            address.to_string()
        )
        .fetch_optional(&mut *conn)
        .await?;

        if let Some(token_id) = token_exists {
            return Ok(token_id.id);
        }

        let token_contract = IUniswapV2ERC20::new(address, self.eth_client.clone());

        // Fallback to "unknown" if the token name or symbol is not available,
        // or it's using a different interface
        let name_call = token_contract.name();
        let symbol_call = token_contract.symbol();
        let decimals_call = token_contract.decimals();
        let (name, symbol, decimals) =
            futures::join!(name_call.call(), symbol_call.call(), decimals_call.call(),);
        let name = name.unwrap_or("unknown".to_string());
        let symbol = symbol.unwrap_or("unknown".to_string());
        let decimals = decimals.unwrap_or(18);

        let token_id = sqlx::query!(
            r#"
            INSERT INTO tokens (address, name, symbol, decimals)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
            address.to_string(),
            name,
            symbol,
            decimals as i32,
        )
        .fetch_one(&mut *conn)
        .await?;

        Ok(token_id.id)
    }

    async fn insert_pair(
        &self,
        conn: &mut PgConnection,
        pair_address: H160,
        number: i32,
        token0_id: i32,
        token1_id: i32,
        factory_id: i32,
    ) -> eyre::Result<i32> {
        let pair_id = sqlx::query!(
            r#"
            INSERT INTO pairs (address, token0, token1, factory, number)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
            pair_address.to_string(),
            token0_id,
            token1_id,
            factory_id,
            number
        )
        .fetch_one(&mut *conn)
        .await?;

        Ok(pair_id.id)
    }

    async fn insert_reserves(
        &self,
        conn: &mut PgConnection,
        pair_id: i32,
        reserve0: u128,
        reserve1: u128,
        block_id: i32,
    ) -> eyre::Result<i32> {
        let reserves_id = sqlx::query!(
            r#"
            INSERT INTO
                reserves (pair, reserve0, reserve1, block)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
            pair_id,
            reserve0 as i64,
            reserve1 as i64,
            block_id
        )
        .fetch_one(&mut *conn)
        .await?;

        Ok(reserves_id.id)
    }
}
