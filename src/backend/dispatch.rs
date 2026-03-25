use super::error::BackendResult;
use super::events::EventReceiver;
#[cfg(feature = "ffi")]
use super::ffi::FfiBackend;
use super::mock::MockBackend;
use super::native::NativeBackend;
use super::r#trait::SpvBackend;
use super::types::{Network, SyncProgress, TransactionInfo, WalletCoreBalance};

/// Enum dispatch wrapper that delegates all `SpvBackend` calls to either
/// a real `NativeBackend`, an in-memory `MockBackend`, or an `FfiBackend`.
#[allow(clippy::large_enum_variant)]
pub enum Backend {
    Native(NativeBackend),
    Mock(MockBackend),
    #[cfg(feature = "ffi")]
    Ffi(FfiBackend),
}

impl SpvBackend for Backend {
    async fn start(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.start().await,
            Self::Mock(b) => b.start().await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.start().await,
        }
    }

    async fn stop(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.stop().await,
            Self::Mock(b) => b.stop().await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.stop().await,
        }
    }

    fn is_running(&self) -> bool {
        match self {
            Self::Native(b) => b.is_running(),
            Self::Mock(b) => b.is_running(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.is_running(),
        }
    }

    fn network(&self) -> Network {
        match self {
            Self::Native(b) => b.network(),
            Self::Mock(b) => b.network(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.network(),
        }
    }

    fn tip_height(&self) -> Option<u32> {
        match self {
            Self::Native(b) => b.tip_height(),
            Self::Mock(b) => b.tip_height(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.tip_height(),
        }
    }

    fn sync_progress(&self) -> SyncProgress {
        match self {
            Self::Native(b) => b.sync_progress(),
            Self::Mock(b) => b.sync_progress(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.sync_progress(),
        }
    }

    fn generate_mnemonic(&self) -> BackendResult<String> {
        match self {
            Self::Native(b) => b.generate_mnemonic(),
            Self::Mock(b) => b.generate_mnemonic(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.generate_mnemonic(),
        }
    }

    async fn create_wallet(&self, mnemonic: &str) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.create_wallet(mnemonic).await,
            Self::Mock(b) => b.create_wallet(mnemonic).await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.create_wallet(mnemonic).await,
        }
    }

    async fn load_wallet(&self) -> BackendResult<bool> {
        match self {
            Self::Native(b) => b.load_wallet().await,
            Self::Mock(b) => b.load_wallet().await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.load_wallet().await,
        }
    }

    fn get_receive_address(&self) -> BackendResult<String> {
        match self {
            Self::Native(b) => b.get_receive_address(),
            Self::Mock(b) => b.get_receive_address(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.get_receive_address(),
        }
    }

    fn get_balance(&self) -> BackendResult<WalletCoreBalance> {
        match self {
            Self::Native(b) => b.get_balance(),
            Self::Mock(b) => b.get_balance(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.get_balance(),
        }
    }

    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>> {
        match self {
            Self::Native(b) => b.get_transactions(),
            Self::Mock(b) => b.get_transactions(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.get_transactions(),
        }
    }

    fn estimate_fee(&self, address: &str, amount: u64, fee_rate: u32) -> BackendResult<u64> {
        match self {
            Self::Native(b) => b.estimate_fee(address, amount, fee_rate),
            Self::Mock(b) => b.estimate_fee(address, amount, fee_rate),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.estimate_fee(address, amount, fee_rate),
        }
    }

    async fn send(&self, address: &str, amount: u64, fee_rate: u32) -> BackendResult<[u8; 32]> {
        match self {
            Self::Native(b) => b.send(address, amount, fee_rate).await,
            Self::Mock(b) => b.send(address, amount, fee_rate).await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.send(address, amount, fee_rate).await,
        }
    }

    fn cache_size(&self) -> BackendResult<u64> {
        match self {
            Self::Native(b) => b.cache_size(),
            Self::Mock(b) => b.cache_size(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.cache_size(),
        }
    }

    async fn clear_cache(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.clear_cache().await,
            Self::Mock(b) => b.clear_cache().await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.clear_cache().await,
        }
    }

    fn subscribe_events(&self) -> EventReceiver {
        match self {
            Self::Native(b) => b.subscribe_events(),
            Self::Mock(b) => b.subscribe_events(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.subscribe_events(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::BackendError;
    use super::super::types::{Network, SyncState};
    use super::*;

    const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn mock_backend(network: Network) -> Backend {
        Backend::Mock(MockBackend::builder(network).build())
    }

    fn mock_backend_with_wallet(network: Network, balance: u64) -> Backend {
        Backend::Mock(
            MockBackend::builder(network)
                .with_confirmed_balance(balance)
                .with_persisted_wallet()
                .build(),
        )
    }

    #[tokio::test]
    async fn dispatch_start_stop_is_running() {
        let backend = mock_backend(Network::Testnet);

        assert!(!backend.is_running());
        backend.start().await.unwrap();
        assert!(backend.is_running());
        backend.stop().await.unwrap();
        assert!(!backend.is_running());
    }

    #[test]
    fn dispatch_network() {
        assert_eq!(mock_backend(Network::Testnet).network(), Network::Testnet);
        assert_eq!(mock_backend(Network::Mainnet).network(), Network::Mainnet);
        assert_eq!(mock_backend(Network::Regtest).network(), Network::Regtest);
    }

    #[test]
    fn dispatch_tip_height() {
        let backend = Backend::Mock(
            MockBackend::builder(Network::Testnet)
                .with_tip_height(12345)
                .build(),
        );
        assert_eq!(backend.tip_height(), Some(12345));

        let backend_no_tip = mock_backend(Network::Testnet);
        assert_eq!(backend_no_tip.tip_height(), None);
    }

    #[test]
    fn dispatch_sync_progress() {
        let backend = mock_backend(Network::Testnet);
        let progress = backend.sync_progress();
        assert_eq!(progress.state(), SyncState::WaitForEvents);
        assert!(!progress.is_synced());
    }

    #[test]
    fn dispatch_generate_mnemonic() {
        let backend = mock_backend(Network::Testnet);
        let mnemonic = backend.generate_mnemonic().unwrap();
        assert_eq!(mnemonic.split_whitespace().count(), 12);
    }

    #[tokio::test]
    async fn dispatch_create_wallet() {
        let backend = mock_backend(Network::Testnet);
        backend.create_wallet(TEST_MNEMONIC).await.unwrap();

        let err = backend.create_wallet(TEST_MNEMONIC).await.unwrap_err();
        assert_eq!(err, BackendError::WalletAlreadyExists);
    }

    #[tokio::test]
    async fn dispatch_load_wallet() {
        let no_wallet = mock_backend(Network::Testnet);
        assert!(!no_wallet.load_wallet().await.unwrap());

        let with_wallet = mock_backend_with_wallet(Network::Testnet, 0);
        assert!(with_wallet.load_wallet().await.unwrap());
    }

    #[tokio::test]
    async fn dispatch_get_receive_address() {
        let backend = mock_backend_with_wallet(Network::Testnet, 100_000);
        backend.load_wallet().await.unwrap();

        let addr = backend.get_receive_address().unwrap();
        assert!(addr.starts_with("XmockAddr"));
    }

    #[tokio::test]
    async fn dispatch_get_balance() {
        let backend = mock_backend_with_wallet(Network::Testnet, 500_000);
        backend.load_wallet().await.unwrap();

        let balance = backend.get_balance().unwrap();
        assert_eq!(balance.spendable(), 500_000);
    }

    #[tokio::test]
    async fn dispatch_get_transactions() {
        let backend = mock_backend_with_wallet(Network::Testnet, 0);
        backend.load_wallet().await.unwrap();

        let txs = backend.get_transactions().unwrap();
        assert!(txs.is_empty());
    }

    #[tokio::test]
    async fn dispatch_estimate_fee() {
        let backend = mock_backend_with_wallet(Network::Testnet, 1_000_000);
        backend.load_wallet().await.unwrap();

        let fee = backend.estimate_fee("Xaddr", 100_000, 1000).unwrap();
        assert!(fee > 0);
    }

    #[tokio::test]
    async fn dispatch_send() {
        let backend = mock_backend_with_wallet(Network::Testnet, 1_000_000);
        backend.load_wallet().await.unwrap();

        let txid = backend.send("Xaddr", 250_000, 1000).await.unwrap();
        assert_ne!(txid, [0u8; 32]);

        let balance = backend.get_balance().unwrap();
        assert_eq!(balance.spendable(), 750_000);

        let txs = backend.get_transactions().unwrap();
        assert_eq!(txs.len(), 1);
    }

    #[tokio::test]
    async fn dispatch_subscribe_events() {
        let backend = mock_backend(Network::Testnet);
        let mut rx = backend.subscribe_events();

        backend.start().await.unwrap();

        let event = rx.try_recv();
        assert!(event.is_ok());
    }

    #[test]
    fn dispatch_network_devnet() {
        assert_eq!(mock_backend(Network::Devnet).network(), Network::Devnet);
    }

    #[tokio::test]
    async fn dispatch_start_already_running() {
        let backend = mock_backend(Network::Testnet);
        backend.start().await.unwrap();
        let err = backend.start().await.unwrap_err();
        assert_eq!(err, BackendError::AlreadyRunning);
    }

    #[tokio::test]
    async fn dispatch_stop_not_running() {
        let backend = mock_backend(Network::Testnet);
        let err = backend.stop().await.unwrap_err();
        assert_eq!(err, BackendError::NotRunning);
    }

    #[test]
    fn dispatch_operations_without_wallet() {
        let backend = mock_backend(Network::Testnet);
        assert_eq!(backend.get_balance().unwrap_err(), BackendError::NoWallet);
        assert_eq!(
            backend.get_receive_address().unwrap_err(),
            BackendError::NoWallet,
        );
        assert_eq!(
            backend.get_transactions().unwrap_err(),
            BackendError::NoWallet,
        );
        assert_eq!(
            backend.estimate_fee("addr", 100, 1000).unwrap_err(),
            BackendError::NoWallet,
        );
    }

    #[tokio::test]
    async fn dispatch_send_errors() {
        let backend = mock_backend_with_wallet(Network::Testnet, 1_000_000);
        backend.load_wallet().await.unwrap();

        // Empty address
        let err = backend.send("", 100, 1000).await.unwrap_err();
        assert!(matches!(err, BackendError::InvalidAddress(_)));

        // Insufficient funds
        let err = backend.send("Xaddr", 2_000_000, 1000).await.unwrap_err();
        assert!(matches!(err, BackendError::InsufficientFunds { .. }));
    }

    #[tokio::test]
    async fn dispatch_create_wallet_invalid_mnemonic() {
        let backend = mock_backend(Network::Testnet);
        let err = backend.create_wallet("bad mnemonic").await.unwrap_err();
        assert!(matches!(err, BackendError::InvalidMnemonic(_)));
    }

    #[tokio::test]
    async fn dispatch_sync_progress_with_tip() {
        let backend = Backend::Mock(
            MockBackend::builder(Network::Testnet)
                .with_tip_height(5000)
                .build(),
        );
        backend.start().await.unwrap();

        let progress = backend.sync_progress();
        assert_eq!(progress.state(), SyncState::WaitForEvents);
    }

    #[tokio::test]
    async fn dispatch_generate_mnemonic_is_12_words() {
        let backend = mock_backend(Network::Regtest);
        let mnemonic = backend.generate_mnemonic().unwrap();
        assert_eq!(mnemonic.split_whitespace().count(), 12);

        // Verify the mnemonic can be used to create a wallet through dispatch
        backend.create_wallet(&mnemonic).await.unwrap();
        assert!(backend.get_balance().is_ok());
    }

    #[tokio::test]
    async fn dispatch_multiple_sends_update_state() {
        let backend = mock_backend_with_wallet(Network::Testnet, 1_000_000);
        backend.load_wallet().await.unwrap();

        backend.send("Xaddr1", 100_000, 1000).await.unwrap();
        backend.send("Xaddr2", 200_000, 1000).await.unwrap();

        let balance = backend.get_balance().unwrap();
        assert_eq!(balance.spendable(), 700_000);

        let txs = backend.get_transactions().unwrap();
        assert_eq!(txs.len(), 2);
    }
}
