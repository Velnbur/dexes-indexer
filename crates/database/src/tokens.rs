use sqlx::FromRow;

/// Entry of the ERC20 token in the database.
#[derive(Debug, Clone, FromRow)]
pub struct TokenEntry {
    /// id of the entry in the database.
    pub id: i32,

    /// Address of the token.
    pub address: String,

    /// Name of the token.
    pub name: String,

    /// Symbol of the token.
    pub symbol: String,

    /// Decimals of the token.
    pub decimals: i32,
}
