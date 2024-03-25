use sqlx::prelude::FromRow;

/// Entry of the `factories` table.
#[derive(Debug, Clone, FromRow)]
pub struct FactoryEntry {
    /// Unique identifier of the factory inside the database.
    pub id: i32,

    /// Address of the factory.
    pub address: String,
}
