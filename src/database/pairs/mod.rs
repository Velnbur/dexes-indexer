use sqlx::prelude::FromRow;

pub trait PairsProvider {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn pair_by_id(&self, pair_id: i32) -> Result<Option<PairEntry>, Self::Error>;

    async fn insert_pair(&self, pair: PairEntry) -> Result<PairEntry, Self::Error>;
}

/// Entry of the `pairs` table.
#[derive(Debug, Clone, FromRow)]
pub struct PairEntry {
    /// Unique identifier of the pair inside the database.
    pub id: i32,
    /// Id of the entry in `factories` table.
    pub factory: i32,

    /// Address of the pair.
    pub address: String,

    pub number: i32,

    /// Address of the token0.
    pub token0: i32,
    /// Address of the token1.
    pub token1: i32,
}
