// Re-export types from rust-dashcore crates.
pub use dash_spv::sync::{ManagerIdentifier, SyncProgress, SyncState};
pub use dashcore::Network;
pub use key_wallet::WalletCoreBalance;
pub use key_wallet::managed_account::transaction_record::TransactionDirection;
pub use key_wallet::transaction_checking::transaction_router::TransactionType;

/// A UI-facing transaction record assembled from wallet events and sync state.
///
/// This combines data from key-wallet's `TransactionRecord` (txid, amount, height,
/// timestamp) with `TransactionContext` from wallet events (IS/CL status).
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionInfo {
    pub txid: dashcore::Txid,
    pub amount: i64,
    pub direction: TransactionDirection,
    pub transaction_type: TransactionType,
    pub timestamp: u64,
    pub height: Option<u32>,
    pub fee: Option<u64>,
    pub addresses: Vec<String>,
    pub block_hash: Option<dashcore::BlockHash>,
    pub is_instant_send: bool,
    pub is_chain_locked: bool,
    pub label: Option<String>,
}

impl TransactionInfo {
    /// Compute confirmations relative to the current chain tip.
    pub fn confirmations(&self, current_height: u32) -> u32 {
        match self.height {
            Some(h) if current_height >= h => current_height - h + 1,
            _ => 0,
        }
    }
}
