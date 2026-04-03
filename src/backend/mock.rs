use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use dashcore::hashes::Hash;
use tokio::sync::broadcast;

use super::error::{BackendError, BackendResult};
use super::events::{EventReceiver, EventSender, SpvEvent};
use super::r#trait::SpvBackend;
use super::types::{
    Network, SyncProgress, TransactionDirection, TransactionInfo, TransactionType,
    WalletCoreBalance,
};

/// Builder for configuring a `MockBackend` instance.
pub struct MockBackendBuilder {
    network: Network,
    balance: WalletCoreBalance,
    transactions: Vec<TransactionInfo>,
    addresses: Vec<String>,
    tip_height: Option<u32>,
    has_persisted_wallet: bool,
}

impl MockBackendBuilder {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            balance: WalletCoreBalance::default(),
            transactions: Vec::new(),
            addresses: vec![mock_address(0)],
            tip_height: None,
            has_persisted_wallet: false,
        }
    }

    pub fn with_balance(mut self, balance: WalletCoreBalance) -> Self {
        self.balance = balance;
        self
    }

    pub fn with_confirmed_balance(mut self, satoshis: u64) -> Self {
        self.balance = WalletCoreBalance::new(satoshis, 0, 0, 0);
        self
    }

    pub fn with_transactions(mut self, transactions: Vec<TransactionInfo>) -> Self {
        self.transactions = transactions;
        self
    }

    pub fn with_tip_height(mut self, height: u32) -> Self {
        self.tip_height = Some(height);
        self
    }

    pub fn with_persisted_wallet(mut self) -> Self {
        self.has_persisted_wallet = true;
        self
    }

    pub fn build(self) -> MockBackend {
        let (event_tx, _) = broadcast::channel(256);
        MockBackend {
            network: self.network,
            running: AtomicBool::new(false),
            wallet_loaded: AtomicBool::new(false),
            has_persisted_wallet: AtomicBool::new(self.has_persisted_wallet),
            tip_height: AtomicU32::new(self.tip_height.unwrap_or(0)),
            balance: Mutex::new(self.balance),
            transactions: Mutex::new(self.transactions),
            addresses: Mutex::new(self.addresses),
            address_index: AtomicU32::new(1),
            event_tx,
        }
    }
}

/// In-memory mock implementation of `SpvBackend` for testing.
pub struct MockBackend {
    network: Network,
    running: AtomicBool,
    wallet_loaded: AtomicBool,
    has_persisted_wallet: AtomicBool,
    tip_height: AtomicU32,
    balance: Mutex<WalletCoreBalance>,
    transactions: Mutex<Vec<TransactionInfo>>,
    addresses: Mutex<Vec<String>>,
    address_index: AtomicU32,
    event_tx: EventSender,
}

impl MockBackend {
    pub fn builder(network: Network) -> MockBackendBuilder {
        MockBackendBuilder::new(network)
    }

    /// Emit an event to all subscribers.
    pub fn emit(&self, event: SpvEvent) {
        let _ = self.event_tx.send(event);
    }

    fn require_wallet(&self) -> BackendResult<()> {
        if !self.wallet_loaded.load(Ordering::Relaxed) {
            return Err(BackendError::NoWallet);
        }
        Ok(())
    }
}

impl SpvBackend for MockBackend {
    async fn start(&self) -> BackendResult<()> {
        if self.running.load(Ordering::Relaxed) {
            return Err(BackendError::AlreadyRunning);
        }
        self.running.store(true, Ordering::Relaxed);
        self.emit(SpvEvent::PeerConnected("mock-peer:9999".to_string()));
        self.emit(SpvEvent::PeersUpdated {
            count: 1,
            best_height: self.tip_height.load(Ordering::Relaxed),
        });
        Ok(())
    }

    async fn stop(&self) -> BackendResult<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(BackendError::NotRunning);
        }
        self.running.store(false, Ordering::Relaxed);
        self.emit(SpvEvent::PeerDisconnected("mock-peer:9999".to_string()));
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn network(&self) -> Network {
        self.network
    }

    fn tip_height(&self) -> Option<u32> {
        let h = self.tip_height.load(Ordering::Relaxed);
        if h == 0 { None } else { Some(h) }
    }

    fn sync_progress(&self) -> SyncProgress {
        SyncProgress::default()
    }

    fn generate_mnemonic(&self) -> BackendResult<String> {
        Ok("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string())
    }

    async fn create_wallet(&self, mnemonic: &str) -> BackendResult<()> {
        if self.wallet_loaded.load(Ordering::Relaxed) {
            return Err(BackendError::WalletAlreadyExists);
        }
        if mnemonic.split_whitespace().count() != 12 && mnemonic.split_whitespace().count() != 24 {
            return Err(BackendError::InvalidMnemonic(
                "mnemonic must be 12 or 24 words".to_string(),
            ));
        }
        self.wallet_loaded.store(true, Ordering::Relaxed);
        self.emit(SpvEvent::BalanceUpdated(WalletCoreBalance::default()));
        Ok(())
    }

    async fn load_wallet(&self) -> BackendResult<bool> {
        if self.has_persisted_wallet.load(Ordering::Relaxed) {
            self.wallet_loaded.store(true, Ordering::Relaxed);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn get_receive_address(&self) -> BackendResult<String> {
        self.require_wallet()?;
        let index = self.address_index.fetch_add(1, Ordering::Relaxed);
        let address = mock_address(index);
        self.addresses.lock().unwrap().push(address.clone());
        Ok(address)
    }

    fn get_balance(&self) -> BackendResult<WalletCoreBalance> {
        self.require_wallet()?;
        Ok(*self.balance.lock().unwrap())
    }

    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>> {
        self.require_wallet()?;
        Ok(self.transactions.lock().unwrap().clone())
    }

    fn estimate_fee(&self, _address: &str, _amount: u64, fee_rate: u32) -> BackendResult<u64> {
        self.require_wallet()?;
        // Estimate for a typical 1-input 2-output P2PKH transaction (226 bytes)
        let estimated_size: u64 = 226;
        let fee = (fee_rate as u64 * estimated_size).div_ceil(1000);
        Ok(fee)
    }

    async fn send(&self, address: &str, amount: u64, _fee_rate: u32) -> BackendResult<[u8; 32]> {
        self.require_wallet()?;

        if address.is_empty() {
            return Err(BackendError::InvalidAddress("empty address".to_string()));
        }

        let mut balance = self.balance.lock().unwrap();
        if balance.spendable() < amount {
            return Err(BackendError::InsufficientFunds {
                available: balance.spendable(),
                required: amount,
            });
        }

        let new_balance = WalletCoreBalance::new(
            balance.spendable() - amount,
            balance.unconfirmed(),
            balance.immature(),
            balance.locked(),
        );
        *balance = new_balance;
        drop(balance);

        let txid_bytes = mock_txid(self.transactions.lock().unwrap().len() as u32);

        let record = TransactionInfo {
            txid: dashcore::Txid::from_byte_array(txid_bytes),
            amount: -(amount as i64),
            direction: TransactionDirection::Outgoing,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: None,
            fee: None,
            addresses: vec![address.to_string()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
        };

        self.transactions.lock().unwrap().push(record.clone());
        self.emit(SpvEvent::BalanceUpdated(new_balance));
        self.emit(SpvEvent::TransactionReceived(Box::new(record)));

        Ok(txid_bytes)
    }

    fn cache_size(&self) -> BackendResult<u64> {
        Ok(0)
    }

    async fn clear_cache(&self) -> BackendResult<()> {
        Ok(())
    }

    fn subscribe_events(&self) -> EventReceiver {
        self.event_tx.subscribe()
    }
}

fn mock_address(index: u32) -> String {
    format!("XmockAddr{index:028}")
}

fn mock_txid(index: u32) -> [u8; 32] {
    let mut txid = [0xFFu8; 32];
    let bytes = index.to_le_bytes();
    txid[..4].copy_from_slice(&bytes);
    txid
}

/// Creates a test transaction record.
pub fn mock_transaction(
    index: u32,
    direction: TransactionDirection,
    amount: u64,
) -> TransactionInfo {
    TransactionInfo {
        txid: dashcore::Txid::from_byte_array(mock_txid(index)),
        amount: match direction {
            TransactionDirection::Outgoing | TransactionDirection::CoinJoin => -(amount as i64),
            TransactionDirection::Incoming | TransactionDirection::Internal => amount as i64,
        },
        direction,
        transaction_type: TransactionType::Standard,
        timestamp: 1700000000 + (index as u64 * 600),
        height: Some(1000 + index),
        fee: None,
        addresses: vec![mock_address(index)],
        block_hash: None,
        is_instant_send: false,
        is_chain_locked: false,
        label: None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::SyncState;
    use super::*;

    const TEST_MNEMONIC_12: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const TEST_MNEMONIC_24: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

    // -- Trait contract tests --

    #[tokio::test]
    async fn lifecycle_start_stop() {
        let backend = MockBackend::builder(Network::Testnet).build();

        assert!(!backend.is_running());
        backend.start().await.unwrap();
        assert!(backend.is_running());
        backend.stop().await.unwrap();
        assert!(!backend.is_running());
    }

    #[tokio::test]
    async fn lifecycle_double_start_errors() {
        let backend = MockBackend::builder(Network::Testnet).build();

        backend.start().await.unwrap();
        let err = backend.start().await.unwrap_err();
        assert_eq!(err, BackendError::AlreadyRunning);
    }

    #[tokio::test]
    async fn lifecycle_stop_when_not_running_errors() {
        let backend = MockBackend::builder(Network::Testnet).build();

        let err = backend.stop().await.unwrap_err();
        assert_eq!(err, BackendError::NotRunning);
    }

    #[tokio::test]
    async fn generate_mnemonic_returns_valid_phrase() {
        let backend = MockBackend::builder(Network::Testnet).build();

        let phrase = backend.generate_mnemonic().unwrap();
        let words: Vec<&str> = phrase.split_whitespace().collect();
        assert_eq!(words.len(), 12);

        // The generated phrase should be accepted by create_wallet
        backend.create_wallet(&phrase).await.unwrap();
    }

    #[tokio::test]
    async fn wallet_create_and_query() {
        let backend = MockBackend::builder(Network::Mainnet)
            .with_confirmed_balance(500_000)
            .build();

        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let address = backend.get_receive_address().unwrap();
        assert!(address.starts_with("XmockAddr"));

        let balance = backend.get_balance().unwrap();
        assert_eq!(balance.spendable(), 500_000);

        let txs = backend.get_transactions().unwrap();
        assert!(txs.is_empty());
    }

    #[tokio::test]
    async fn wallet_create_with_24_words() {
        let backend = MockBackend::builder(Network::Testnet).build();
        backend.create_wallet(TEST_MNEMONIC_24).await.unwrap();
        assert!(backend.get_balance().is_ok());
    }

    #[tokio::test]
    async fn wallet_create_invalid_mnemonic() {
        let backend = MockBackend::builder(Network::Testnet).build();

        let err = backend.create_wallet("only three words").await.unwrap_err();
        assert!(matches!(err, BackendError::InvalidMnemonic(_)));
    }

    #[tokio::test]
    async fn wallet_create_duplicate_errors() {
        let backend = MockBackend::builder(Network::Testnet).build();

        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();
        let err = backend.create_wallet(TEST_MNEMONIC_12).await.unwrap_err();
        assert_eq!(err, BackendError::WalletAlreadyExists);
    }

    #[tokio::test]
    async fn wallet_operations_without_wallet_error() {
        let backend = MockBackend::builder(Network::Testnet).build();

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
            backend.send("addr", 100, 1000).await.unwrap_err(),
            BackendError::NoWallet,
        );
    }

    #[tokio::test]
    async fn send_success() {
        let backend = MockBackend::builder(Network::Testnet)
            .with_confirmed_balance(1_000_000)
            .build();
        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let txid = backend.send("XrecipientAddr", 250_000, 1000).await.unwrap();
        assert_ne!(txid, [0u8; 32]);

        let balance = backend.get_balance().unwrap();
        assert_eq!(balance.spendable(), 750_000);

        let txs = backend.get_transactions().unwrap();
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0].direction, TransactionDirection::Outgoing);
        assert_eq!(txs[0].amount, -250_000);
    }

    #[tokio::test]
    async fn send_insufficient_funds() {
        let backend = MockBackend::builder(Network::Testnet)
            .with_confirmed_balance(100)
            .build();
        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let err = backend.send("XrecipientAddr", 200, 1000).await.unwrap_err();
        assert_eq!(
            err,
            BackendError::InsufficientFunds {
                available: 100,
                required: 200,
            },
        );
    }

    #[tokio::test]
    async fn send_invalid_address() {
        let backend = MockBackend::builder(Network::Testnet)
            .with_confirmed_balance(1_000_000)
            .build();
        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let err = backend.send("", 100, 1000).await.unwrap_err();
        assert!(matches!(err, BackendError::InvalidAddress(_)));
    }

    #[tokio::test]
    async fn load_wallet_with_persisted() {
        let backend = MockBackend::builder(Network::Testnet)
            .with_persisted_wallet()
            .build();

        let found = backend.load_wallet().await.unwrap();
        assert!(found);
        assert!(backend.get_balance().is_ok());
    }

    #[tokio::test]
    async fn load_wallet_without_persisted() {
        let backend = MockBackend::builder(Network::Testnet).build();

        let found = backend.load_wallet().await.unwrap();
        assert!(!found);
    }

    #[tokio::test]
    async fn network_and_tip_height() {
        let backend = MockBackend::builder(Network::Regtest)
            .with_tip_height(42000)
            .build();

        assert_eq!(backend.network(), Network::Regtest);
        assert_eq!(backend.tip_height(), Some(42000));
    }

    #[tokio::test]
    async fn tip_height_none_when_zero() {
        let backend = MockBackend::builder(Network::Mainnet).build();
        assert_eq!(backend.tip_height(), None);
    }

    #[tokio::test]
    async fn sync_progress_default_when_not_running() {
        let backend = MockBackend::builder(Network::Testnet).build();

        let progress = backend.sync_progress();
        assert_eq!(progress.state(), SyncState::WaitForEvents);
        assert!(!progress.is_synced());
    }

    #[tokio::test]
    async fn sync_progress_default_when_running() {
        let backend = MockBackend::builder(Network::Testnet)
            .with_tip_height(1000)
            .build();
        backend.start().await.unwrap();

        let progress = backend.sync_progress();
        assert_eq!(progress.state(), SyncState::WaitForEvents);
        assert!(!progress.is_synced());
    }

    // -- Event tests --

    #[tokio::test]
    async fn events_delivered_on_start() {
        let backend = MockBackend::builder(Network::Testnet)
            .with_tip_height(5000)
            .build();
        let mut rx = backend.subscribe_events();

        backend.start().await.unwrap();

        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, SpvEvent::PeerConnected(_)));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, SpvEvent::PeersUpdated { count: 1, .. }));
    }

    #[tokio::test]
    async fn events_delivered_on_send() {
        let backend = MockBackend::builder(Network::Testnet)
            .with_confirmed_balance(1_000_000)
            .build();
        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let mut rx = backend.subscribe_events();
        // Drain the BalanceUpdated from create_wallet
        let _ = rx.try_recv();

        backend.send("Xaddr", 500_000, 1000).await.unwrap();

        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, SpvEvent::BalanceUpdated(_)));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, SpvEvent::TransactionReceived(_)));
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_events() {
        let backend = MockBackend::builder(Network::Testnet).build();
        let mut rx1 = backend.subscribe_events();
        let mut rx2 = backend.subscribe_events();

        backend.start().await.unwrap();

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    // -- Builder tests --

    #[tokio::test]
    async fn builder_with_transactions() {
        let txs = vec![
            mock_transaction(0, TransactionDirection::Incoming, 100_000),
            mock_transaction(1, TransactionDirection::Outgoing, 50_000),
        ];
        let backend = MockBackend::builder(Network::Mainnet)
            .with_transactions(txs.clone())
            .build();
        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let result = backend.get_transactions().unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].direction, TransactionDirection::Incoming);
        assert_eq!(result[1].direction, TransactionDirection::Outgoing);
    }

    #[tokio::test]
    async fn builder_with_balance() {
        let balance = WalletCoreBalance::new(1_000_000, 50_000, 0, 25_000);
        let backend = MockBackend::builder(Network::Testnet)
            .with_balance(balance)
            .build();
        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let result = backend.get_balance().unwrap();
        assert_eq!(result, balance);
        assert_eq!(result.total(), 1_075_000);
        assert_eq!(result.spendable(), 1_000_000);
    }

    #[tokio::test]
    async fn receive_address_increments() {
        let backend = MockBackend::builder(Network::Testnet).build();
        backend.create_wallet(TEST_MNEMONIC_12).await.unwrap();

        let addr1 = backend.get_receive_address().unwrap();
        let addr2 = backend.get_receive_address().unwrap();
        assert_ne!(addr1, addr2);
    }

    #[test]
    fn cache_size_returns_zero() {
        let backend = MockBackend::builder(Network::Testnet).build();
        assert_eq!(backend.cache_size().unwrap(), 0);
    }

    #[tokio::test]
    async fn clear_cache_is_noop() {
        let backend = MockBackend::builder(Network::Testnet).build();
        backend.clear_cache().await.unwrap();
    }
}
