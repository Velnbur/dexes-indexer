use sqlx::types::BigDecimal;

#[derive(Debug, Clone)]
pub struct ReserveEntry {
    pub id: i32,

    pub pair: i32,
    pub block: i32,

    pub reserve0: BigDecimal,
    pub reserve1: BigDecimal,
}
