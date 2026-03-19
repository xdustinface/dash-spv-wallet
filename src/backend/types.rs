use std::fmt;

use serde::{Deserialize, Serialize};

/// Supported networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Network {
    Mainnet,
    Testnet,
    Regtest,
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mainnet => write!(f, "Mainnet"),
            Self::Testnet => write!(f, "Testnet"),
            Self::Regtest => write!(f, "Regtest"),
        }
    }
}

/// Wallet balance in satoshis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Balance {
    pub confirmed: u64,
    pub pending: u64,
    pub immature: u64,
    pub locked: u64,
}

impl Balance {
    pub fn total(&self) -> u64 {
        self.confirmed + self.pending + self.immature + self.locked
    }

    pub fn spendable(&self) -> u64 {
        self.confirmed
    }
}

/// Direction of a transaction relative to the wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionDirection {
    Sent,
    Received,
}

/// A transaction record as seen by the wallet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub txid: [u8; 32],
    pub amount: i64,
    pub direction: TransactionDirection,
    pub timestamp: u64,
    pub confirmations: u32,
    pub addresses: Vec<String>,
    pub is_instant_send: bool,
    pub is_chain_locked: bool,
}

/// Identifies a sync manager/pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManagerId {
    Headers,
    FilterHeaders,
    Filters,
    Blocks,
    Masternodes,
    ChainLocks,
    InstantSend,
}

impl fmt::Display for ManagerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Headers => write!(f, "Headers"),
            Self::FilterHeaders => write!(f, "Filter Headers"),
            Self::Filters => write!(f, "Filters"),
            Self::Blocks => write!(f, "Blocks"),
            Self::Masternodes => write!(f, "Masternodes"),
            Self::ChainLocks => write!(f, "ChainLocks"),
            Self::InstantSend => write!(f, "InstantSend"),
        }
    }
}

/// Current state of synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncState {
    WaitForEvents,
    WaitingForConnections,
    Syncing,
    Synced,
    Error,
}

/// Progress for a single sync manager.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagerProgress {
    pub manager: ManagerId,
    pub state: SyncState,
    pub current_height: u32,
    pub target_height: u32,
    pub percentage: f64,
}

/// Aggregated sync progress across all managers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncProgress {
    pub state: SyncState,
    pub percentage: f64,
    pub is_synced: bool,
    pub managers: Vec<ManagerProgress>,
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self {
            state: SyncState::WaitForEvents,
            percentage: 0.0,
            is_synced: false,
            managers: Vec::new(),
        }
    }
}
