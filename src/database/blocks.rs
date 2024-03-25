#[derive(Debug, Clone)]
pub struct BlockEntry {
    /// Id of the block inside database
    pub id: i32,

    /// Height of the block on chain
    pub height: i32,

    /// Hash of the block
    pub hash: String,
}
