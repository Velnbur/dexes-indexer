use sqlx::{types::BigDecimal, FromRow};

#[derive(Debug, Clone, FromRow)]
pub struct ReserveEntry {
    pub id: i32,

    pub pair: i32,
    pub block: i32,

    pub reserve0: BigDecimal,
    pub reserve1: BigDecimal,
}
