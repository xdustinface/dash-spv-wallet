// Re-export types from rust-dashcore crates.
pub use dash_spv::sync::{ManagerIdentifier, SyncProgress, SyncState};
pub use dashcore::Network;
pub use key_wallet::WalletCoreBalance;
pub use key_wallet::managed_account::transaction_record::TransactionDirection;
pub use key_wallet::transaction_checking::transaction_router::TransactionType;

/// Role of a transaction output from the wallet's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputRole {
    /// Output to our external/receive address.
    Received,
    /// Output to our internal/change address.
    Change,
    /// Output to counterparty address.
    Sent,
    /// Unspendable output (OP_RETURN, non-standard, bare multisig).
    Unspendable,
}

/// Wallet-context metadata for a transaction input.
#[derive(Debug, Clone, PartialEq)]
pub struct InputInfo {
    /// Index into the transaction's input array.
    pub index: u32,
    /// Value of the UTXO being spent (satoshis).
    pub value: u64,
    /// Address that owned the spent UTXO.
    pub address: String,
}

/// Wallet-context metadata for a transaction output.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputInfo {
    /// Index into the transaction's output array.
    pub index: u32,
    /// Value of this output (satoshis).
    pub value: u64,
    /// Address receiving this output.
    pub address: String,
    /// Role from the wallet's perspective.
    pub role: OutputRole,
}

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
    pub inputs: Vec<InputInfo>,
    pub outputs: Vec<OutputInfo>,
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
