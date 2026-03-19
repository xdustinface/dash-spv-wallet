use super::error::BackendResult;
use super::events::EventReceiver;
use super::types::{Balance, Network, SyncProgress, TransactionRecord};

/// Abstraction over the SPV client, enabling both native Rust and FFI backends.
///
/// All UI code interacts with the SPV client exclusively through this trait.
/// Implementations must be thread-safe (`Send + Sync`).
pub trait SpvBackend: Send + Sync + 'static {
    // -- Lifecycle --

    /// Start the SPV client and begin syncing.
    fn start(&self) -> impl Future<Output = BackendResult<()>> + Send;

    /// Stop the SPV client and all sync activity.
    fn stop(&self) -> impl Future<Output = BackendResult<()>> + Send;

    /// Pause syncing but keep the client alive.
    fn pause(&self) -> impl Future<Output = BackendResult<()>> + Send;

    /// Resume syncing after a pause.
    fn resume(&self) -> impl Future<Output = BackendResult<()>> + Send;

    /// Whether the client is currently running (started and not stopped).
    fn is_running(&self) -> bool;

    // -- Configuration --

    /// The network this backend is configured for.
    fn network(&self) -> Network;

    /// Current chain tip height, if known.
    fn tip_height(&self) -> Option<u32>;

    /// Current sync progress snapshot.
    fn sync_progress(&self) -> SyncProgress;

    // -- Wallet --

    /// Create a new wallet from a mnemonic phrase.
    fn create_wallet(&self, mnemonic: &str) -> impl Future<Output = BackendResult<()>> + Send;

    /// Load a previously persisted wallet. Returns `true` if a wallet was found.
    fn load_wallet(&self) -> impl Future<Output = BackendResult<bool>> + Send;

    /// Generate and return the next unused receive address.
    fn get_receive_address(&self) -> BackendResult<String>;

    /// Get the current wallet balance.
    fn get_balance(&self) -> BackendResult<Balance>;

    /// Get the wallet's transaction history.
    fn get_transactions(&self) -> BackendResult<Vec<TransactionRecord>>;

    /// Send funds to an address. Returns the transaction ID.
    fn send(&self, address: &str, amount: u64)
        -> impl Future<Output = BackendResult<[u8; 32]>> + Send;

    // -- Events --

    /// Subscribe to backend events. Multiple subscribers are supported.
    fn subscribe_events(&self) -> EventReceiver;
}
