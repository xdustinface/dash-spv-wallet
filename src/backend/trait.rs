use super::error::BackendResult;
use super::events::EventReceiver;
use super::types::{Network, SyncProgress, TransactionInfo, WalletCoreBalance};

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

    /// Generate a new BIP-39 mnemonic phrase.
    fn generate_mnemonic(&self) -> BackendResult<String>;

    /// Create a new wallet from a mnemonic phrase.
    fn create_wallet(&self, mnemonic: &str) -> impl Future<Output = BackendResult<()>> + Send;

    /// Load a previously persisted wallet. Returns `true` if a wallet was found.
    fn load_wallet(&self) -> impl Future<Output = BackendResult<bool>> + Send;

    /// Generate and return the next unused receive address.
    fn get_receive_address(&self) -> BackendResult<String>;

    /// Get the current wallet balance.
    fn get_balance(&self) -> BackendResult<WalletCoreBalance>;

    /// Get the wallet's transaction history.
    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>>;

    /// Estimate the fee for a transaction to the given address and amount.
    fn estimate_fee(&self, address: &str, amount: u64, fee_rate: u32) -> BackendResult<u64>;

    /// Send funds to an address. Returns the transaction ID.
    fn send(
        &self,
        address: &str,
        amount: u64,
        fee_rate: u32,
    ) -> impl Future<Output = BackendResult<[u8; 32]>> + Send;

    // -- Labels --

    /// Set or clear a label on a transaction.
    ///
    /// An empty string clears the label. The label must not exceed 256 bytes.
    fn set_transaction_label(
        &self,
        txid: &str,
        label: &str,
    ) -> impl Future<Output = BackendResult<()>> + Send;

    // -- Cache --

    /// Returns the total size in bytes of cached SPV data (headers, filters, blocks, etc.).
    fn cache_size(&self) -> BackendResult<u64>;

    /// Stops the client, deletes all cached SPV data, and prepares for a fresh sync.
    /// Preserves wallet data (mnemonic) and configuration.
    fn clear_cache(&self) -> impl Future<Output = BackendResult<()>> + Send;

    // -- Events --

    /// Subscribe to backend events. Multiple subscribers are supported.
    fn subscribe_events(&self) -> EventReceiver;
}
