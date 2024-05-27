use std::collections::HashMap;

use color_eyre::eyre::{self, Context};
use config::Config;
use database::{tokens::TokenEntry, PairsStreamEntry, DB};
use ethers::{abi::Address, providers::StreamExt};
use petgraph::{
    prelude::{GraphMap, UnGraphMap},
    Undirected,
};
use sqlx::{types::BigDecimal, PgConnection};

use crate::cli::FindArgs;

pub type ShortAddress = String;

pub struct BaseTokenInfo {
    pub address: Address,
    pub reserves: HashMap<ShortAddress, (BigDecimal, BigDecimal)>,
}

pub async fn find(config: Config, args: FindArgs) -> eyre::Result<()> {
    let database = DB::from_url(&config.database.url).await?;

    let mut graph = UnGraphMap::new();

    let mut txn = database.pool().begin().await?;

    fill_graph_from_db(&mut txn, &mut graph, args.base_token).await?;

    let start_token = get_token(&mut txn, args.from).await?;
    let goal_token = get_token(&mut txn, args.to).await?;

    petgraph::algo::dijkstra(
        &graph,
        start_token.id,
        Some(goal_token.id),
        |(node_a, node_b, (reserve0, reserve1))| todo!(),
    );

    Ok(())
}

async fn fill_graph_from_db(
    conn: &mut PgConnection,
    graph: &mut GraphMap<i32, (BigDecimal, BigDecimal), Undirected>,
    base_token: Address,
) -> eyre::Result<BaseTokenInfo> {
    let mut pairs_stream = DB::pairs_stream(conn).await?;

    let base_token_info = BaseTokenInfo {
        address: base_token,
        reserves: HashMap::new(),
    };

    let base_token_short_address = base_token.to_string();

    while let Some(result) = pairs_stream.next().await {
        let pair = result.wrap_err("Failed to get pair from database")?;

        if [pair.token0_short_address, pair.token1_short_address].contains(&base_token_short_address) {
            let token0 = get_token_by_id(conn, pair.token0_id).await?;
            let token1 = get_token_by_id(conn, pair.token1_id).await?;

            let (reserve0, reserve1, address) = if pair.token0_short_address == base_token_short_address {
                (pair.reserve0, pair.reserve1, token1.address)
            } else {
                (pair.reserve1, pair.reserve0, token0.address)
            };

            base_token_info
                .reserves
                .insert(address, (reserve0, reserve1));
        }

        graph.add_edge(
            pair.token0_id,
            pair.token1_id,
            (pair.reserve0, pair.reserve1),
        );
    }

    Ok(())
}

async fn get_token(
    conn: &mut PgConnection,
    address: ethers::types::Address,
) -> eyre::Result<TokenEntry> {
    let token = DB::token_by_address(&mut *conn, address)
        .await?
        .ok_or_else(|| eyre::eyre!("Token not found {}", address))?;

    Ok(token)
}

async fn get_token_by_id(
    conn: &mut PgConnection,
    id: i32,
) -> eyre::Result<TokenEntry> {
    let token = DB::token_by_id(&mut *conn, id)
        .await?
        .ok_or_else(|| eyre::eyre!("Token not found with id: {}", id))?;

    Ok(token)
}

/// Calculate the amount of tokens to swap on Uniswap
///
/// Formula:
///
/// $$
/// \text{amountOut} = \frac{\text{amountIn} \times \text{reserveOut} \times 997}{\text{reserveIn} \times 1000 + \text{amountIn} \times 997}
/// $$
fn calculate_uniswap_swap_amount(
    from_amount: BigDecimal,
    from_reserve: BigDecimal,
    to_reserve: BigDecimal,
) -> BigDecimal {
    let numerator = from_amount * to_reserve * BigDecimal::from(997);
    let denominator = from_reserve * BigDecimal::from(1000) + from_amount * BigDecimal::from(997);

    numerator / denominator
}
