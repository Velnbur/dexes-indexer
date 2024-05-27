use std::{env, pin::Pin};

use ethers::{
    abi::Hash,
    types::{Address, Block},
};
use eyre::Context;
use futures::Stream;
use pairs::PairEntry;
use sqlx::{PgConnection, FromRow, types::BigDecimal};

pub mod blocks;
pub mod factories;
pub mod pairs;
pub mod reserves;
pub mod tokens;

pub type AsyncStream<'a, T> = Pin<Box<dyn Stream<Item = Result<T, sqlx::Error>> + Send + 'a>>;

#[derive(Clone)]
pub struct DB {
    pool: sqlx::PgPool,
}

impl DB {
    /// Default constructor for the Database struct used usually for testing
    pub async fn new() -> eyre::Result<Self> {
        let env = env::var("DATABASE_URL").wrap_err("DATABASE_URL must be set")?;

        Self::from_url(&env).await
    }

    pub async fn from_url(url: &str) -> eyre::Result<Self> {
        let pool = sqlx::PgPool::connect(url).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    /// Insert block entry into the database and return it's id.
    pub async fn insert_block(&self, block: Block<Hash>) -> eyre::Result<i32> {
        let height = block
            .number
            .ok_or_else(|| eyre::eyre!("Block number is missing"))?
            .as_u64() as i64;
        let hash = block
            .hash
            .ok_or_else(|| eyre::eyre!("Block hash is missing"))?
            .to_string();

        let block_record = sqlx::query!(
            r#"
            INSERT INTO blocks (height, hash)
            VALUES ($1, $2)
            RETURNING id
            "#,
            height,
            hash
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(block_record.id)
    }

    /// Return number of the last indexed pair in the database for given
    /// factory.
    pub async fn last_indexed_pair(&self, factory_id: i32) -> eyre::Result<i32> {
        let last_indexed_pair = sqlx::query!(
            r#"
            SELECT number
            FROM pairs
            WHERE factory = $1
            ORDER BY number DESC
            LIMIT 1
            "#,
            factory_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(last_indexed_pair.number)
    }

    /// Insert factory entry into the database and return it's id.
    ///
    /// And duplicated addresses, do nothing and return the previous id.
    pub async fn insert_factory(&self, factory: Address) -> eyre::Result<i32> {
        let factory_record = sqlx::query!(
            r#"
            INSERT INTO factories (address)
            VALUES ($1)
            ON CONFLICT (address)
            DO UPDATE
                SET address = EXCLUDED.address
            RETURNING id
            "#,
            factory.to_string(),
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(factory_record.id)
    }

    /// For two tokens, check if they exist in the database.
    ///
    /// Return the ids of the tokens if they exist.
    pub async fn check_tokens_exist(
        conn: &mut PgConnection,
        token0: Address,
        token1: Address,
    ) -> eyre::Result<(Option<i32>, Option<i32>)> {
        let token0_record = sqlx::query!(
            r#"
            SELECT id
            FROM tokens
            WHERE address = $1
            "#,
            token0.to_string(),
        )
        .fetch_optional(&mut *conn)
        .await?;

        let token1_record = sqlx::query!(
            r#"
            SELECT id
            FROM tokens
            WHERE address = $1
            "#,
            token1.to_string(),
        )
        .fetch_optional(&mut *conn)
        .await?;

        Ok((token0_record.map(|r| r.id), token1_record.map(|r| r.id)))
    }

    /// Insert token entry into the database and return it's id.
    ///
    /// And duplicated addresses, do nothing and return the previous id.
    pub async fn insert_token(
        conn: &mut PgConnection,
        address: Address,
        name: String,
        symbol: String,
        decimals: u8,
    ) -> eyre::Result<i32> {
        let token_record = sqlx::query!(
            r#"
            INSERT INTO
                tokens (address, name, symbol, decimals)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (address)
            DO UPDATE
                SET address = EXCLUDED.address
            RETURNING id
            "#,
            address.to_string(),
            name,
            symbol,
            decimals as i32,
        )
        .fetch_one(&mut *conn)
        .await?;

        Ok(token_record.id)
    }

    /// Insert pair entry into the database and return it's id.
    pub async fn insert_pair(
        conn: &mut PgConnection,
        pair_address: Address,
        number: i32,
        token0_id: i32,
        token1_id: i32,
        factory_id: i32,
    ) -> eyre::Result<i32> {
        let pair_record = sqlx::query!(
            r#"
            INSERT INTO
                pairs (address, token0, token1, factory, number)
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

        Ok(pair_record.id)
    }

    /// Insert reserve entry into the database and return it's id.
    ///
    /// NOTE: The reserves are stored in the database as i64, but the input is u128,
    /// so the conversion is done here.
    pub async fn insert_reserves(
        conn: &mut PgConnection,
        pair_id: i32,
        reserve0: u128,
        reserve1: u128,
        block_id: i32,
    ) -> eyre::Result<i32> {
        let reserve_record = sqlx::query!(
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

        Ok(reserve_record.id)
    }

    pub async fn first_factory_id(
        conn: &mut PgConnection,
    ) -> eyre::Result<i32> {
        let factory_record = sqlx::query!(
            r#"
            SELECT id
            FROM factories
            ORDER BY id
            LIMIT 1
            "#,
        )
        .fetch_one(&mut *conn)
        .await?;

        Ok(factory_record.id)
    }

    pub async fn pairs_stream<'a>(
        conn: &'a mut PgConnection,
    ) -> eyre::Result<AsyncStream<'a, PairsStreamEntry>> {
        let factory_id = Self::first_factory_id(conn).await?;

        let stream = sqlx::query_as!(
            PairsStreamEntry,
            r#"--sql
            SELECT
                pairs.id as pair_id,
                tokens0.id as token0_id,
                tokens0.address as token0_short_address,
                tokens1.id as token1_id,
                tokens1.address as token1_short_address,
                reserves.reserve0 as reserve0,
                reserves.reserve1 as reserve1
            FROM pairs
            JOIN tokens as tokens0 ON pairs.token0 = tokens0.id
            JOIN tokens as tokens1 ON pairs.token1 = tokens1.id
            JOIN reserves ON pairs.id = reserves.pair AND reserves.block = (
                SELECT blocks.id
                FROM blocks
                ORDER BY blocks.id DESC
                LIMIT 1
            )
            WHERE factory = $1
            "#,
            factory_id,
        )
        .fetch(conn);

        Ok(stream)
    }

    pub async fn pair_by_address(
        conn: &mut PgConnection,
        address: Address
    ) -> eyre::Result<Option<PairEntry>> {
        let pair = sqlx::query_as!(
            PairEntry,
            r#"
            SELECT *
            FROM pairs
            WHERE address = $1
            "#,
            address.to_string(),
        )
        .fetch_optional(conn)
        .await?;

        Ok(pair)
    }

    pub async fn token_by_address(
        conn: &mut PgConnection,
        address: Address
    ) -> eyre::Result<Option<tokens::TokenEntry>> {
        let token = sqlx::query_as!(
            tokens::TokenEntry,
            r#"
            SELECT *
            FROM tokens
            WHERE address = $1
            "#,
            address.to_string(),
        )
        .fetch_optional(conn)
        .await?;

        Ok(token)
    }

    pub async fn token_by_id(
        conn: &mut PgConnection,
        id: i32
    ) -> eyre::Result<Option<tokens::TokenEntry>> {
        let token = sqlx::query_as!(
            tokens::TokenEntry,
            r#"
            SELECT *
            FROM tokens
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(conn)
        .await?;

        Ok(token)
    }
}

#[derive(Debug, FromRow)]
pub struct PairsStreamEntry {
    /// Unique identifier of the pair inside the database.
    pub pair_id: i32,

    pub token0_id: i32,
    pub token0_short_address: String,
    pub token1_id: i32,
    pub token1_short_address: String,

    /// Amount of token0 in the pair.
    pub reserve0: BigDecimal,

    /// Amount of token1 in the pair.
    pub reserve1: BigDecimal,
}
