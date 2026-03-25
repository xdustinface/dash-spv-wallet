use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use std::path::PathBuf;

use dash_spv::network::NetworkEvent;
use dash_spv::network::manager::PeerNetworkManager;
use dash_spv::storage::DiskStorageManager;
use dash_spv::sync::SyncEvent;
use dash_spv::{ClientConfig, DashSpvClient};
use dashcore::hashes::Hash;
use key_wallet::managed_account::managed_account_type::ManagedAccountType;
use key_wallet::mnemonic::Language;
use key_wallet::wallet::initialization::WalletAccountCreationOptions;
use key_wallet::wallet::managed_wallet_info::ManagedWalletInfo;
use key_wallet::wallet::managed_wallet_info::coin_selection::SelectionError;
use key_wallet::wallet::managed_wallet_info::transaction_builder::BuilderError;
use key_wallet::wallet::managed_wallet_info::transaction_building::AccountTypePreference;
use key_wallet::wallet::managed_wallet_info::wallet_info_interface::WalletInfoInterface;
use key_wallet::{DerivationPathBuilder, Mnemonic};
use key_wallet_manager::{
    FeeRate, SelectionStrategy, TransactionBuilder, WalletEvent, WalletManager,
};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::error::{BackendError, BackendResult};
use super::events::{EventReceiver, EventSender, SpvEvent, event_channel};
use super::r#trait::SpvBackend;
use super::types::{
    Network, SyncProgress, TransactionDirection, TransactionInfo, WalletCoreBalance,
};
use crate::config::AppConfig;

type SpvClient =
    DashSpvClient<WalletManager<ManagedWalletInfo>, PeerNetworkManager, DiskStorageManager>;

pub struct NativeBackend {
    config: AppConfig,
    wallet: Arc<tokio::sync::RwLock<WalletManager<ManagedWalletInfo>>>,
    running: AtomicBool,
    event_tx: EventSender,
    progress: Arc<RwLock<SyncProgress>>,
    shutdown_token: tokio::sync::Mutex<Option<CancellationToken>>,
    client_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    bridge_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    client: tokio::sync::Mutex<Option<Arc<SpvClient>>>,
}

impl NativeBackend {
    pub fn new(config: AppConfig) -> Self {
        let wallet_manager = WalletManager::<ManagedWalletInfo>::new(config.network);
        let wallet = Arc::new(tokio::sync::RwLock::new(wallet_manager));
        let (event_tx, _) = event_channel(4096);

        Self {
            config,
            wallet,
            running: AtomicBool::new(false),
            event_tx,
            progress: Arc::new(RwLock::new(SyncProgress::default())),
            shutdown_token: tokio::sync::Mutex::new(None),
            client_task: tokio::sync::Mutex::new(None),
            bridge_task: tokio::sync::Mutex::new(None),
            client: tokio::sync::Mutex::new(None),
        }
    }
}

impl NativeBackend {
    fn mnemonic_path(&self) -> PathBuf {
        self.config.wallet_dir().join("wallet.mnemonic")
    }

    fn save_mnemonic(&self, mnemonic: &str) -> BackendResult<()> {
        let path = self.mnemonic_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BackendError::Storage(e.to_string()))?;
        }
        std::fs::write(&path, mnemonic).map_err(|e| BackendError::Storage(e.to_string()))
    }

    fn read_mnemonic(&self) -> BackendResult<Option<String>> {
        let path = self.mnemonic_path();
        if !path.exists() {
            return Ok(None);
        }
        let contents =
            std::fs::read_to_string(&path).map_err(|e| BackendError::Storage(e.to_string()))?;
        let trimmed = contents.trim().to_string();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed))
        }
    }
}

impl SpvBackend for NativeBackend {
    async fn start(&self) -> BackendResult<()> {
        if self.running.load(Ordering::Relaxed) {
            return Err(BackendError::AlreadyRunning);
        }

        let mut client_config = ClientConfig::new(self.config.network)
            .with_storage_path(self.config.data_dir.clone())
            .with_user_agent("dash-spv-ui");

        if self.config.network == Network::Regtest {
            client_config = client_config.without_masternodes();
        }

        if !self.config.peers.is_empty() {
            client_config.peers.clear();
            for peer in &self.config.peers {
                if let Ok(addr) = peer.parse::<SocketAddr>() {
                    client_config.add_peer(addr);
                }
            }
            client_config = client_config.with_restrict_to_configured_peers(true);
        }

        let network_manager = PeerNetworkManager::new(&client_config)
            .await
            .map_err(|e| BackendError::Sync(e.to_string()))?;

        let storage_manager = DiskStorageManager::new(&client_config)
            .await
            .map_err(|e| BackendError::Storage(e.to_string()))?;

        let client = Arc::new(
            SpvClient::new(
                client_config,
                network_manager,
                storage_manager,
                self.wallet.clone(),
            )
            .await
            .map_err(|e| BackendError::Internal(e.to_string()))?,
        );

        let progress_rx = client.subscribe_progress().await;
        let sync_rx = client.subscribe_sync_events().await;
        let network_rx = client.subscribe_network_events().await;
        let wallet_rx = self.wallet.read().await.subscribe_events();

        let token = CancellationToken::new();

        // Spawn event bridge task
        let bridge_token = token.clone();
        let event_tx = self.event_tx.clone();
        let cached_progress = self.progress.clone();
        let bridge_handle = tokio::spawn(event_bridge(
            bridge_token,
            progress_rx,
            sync_rx,
            network_rx,
            wallet_rx,
            event_tx,
            cached_progress,
        ));

        // Spawn client run task
        let run_token = token.clone();
        let run_client = client.clone();
        let client_handle = tokio::spawn(async move {
            if let Err(e) = run_client.run(run_token).await {
                tracing::error!("SPV client error: {e}");
            }
        });

        *self.client.lock().await = Some(client);
        *self.shutdown_token.lock().await = Some(token);
        *self.client_task.lock().await = Some(client_handle);
        *self.bridge_task.lock().await = Some(bridge_handle);
        self.running.store(true, Ordering::Relaxed);

        Ok(())
    }

    async fn stop(&self) -> BackendResult<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(BackendError::NotRunning);
        }

        if let Some(token) = self.shutdown_token.lock().await.take() {
            token.cancel();
        }

        if let Some(handle) = self.client_task.lock().await.take() {
            let _ = handle.await;
        }

        if let Some(handle) = self.bridge_task.lock().await.take() {
            let _ = handle.await;
        }

        *self.client.lock().await = None;
        self.running.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn network(&self) -> Network {
        self.config.network
    }

    fn tip_height(&self) -> Option<u32> {
        let progress = self.progress.read().ok()?;
        let headers = progress.headers().ok()?;
        let height = headers.tip_height();
        if height == 0 { None } else { Some(height) }
    }

    fn sync_progress(&self) -> SyncProgress {
        self.progress.read().map(|p| p.clone()).unwrap_or_default()
    }

    fn generate_mnemonic(&self) -> BackendResult<String> {
        let mnemonic = Mnemonic::generate(12, Language::English)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        Ok(mnemonic.phrase())
    }

    async fn create_wallet(&self, mnemonic: &str) -> BackendResult<()> {
        let mut wallet = self.wallet.write().await;
        if wallet.wallet_count() > 0 {
            return Err(BackendError::WalletAlreadyExists);
        }
        wallet
            .create_wallet_from_mnemonic(
                mnemonic,
                "",
                0,
                WalletAccountCreationOptions::SpecificAccounts(
                    BTreeSet::from([0]),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    None,
                ),
            )
            .map_err(|e| match e {
                key_wallet_manager::WalletError::InvalidMnemonic(msg) => {
                    BackendError::InvalidMnemonic(msg)
                }
                key_wallet_manager::WalletError::WalletExists(_) => {
                    BackendError::WalletAlreadyExists
                }
                other => BackendError::Internal(other.to_string()),
            })?;
        self.save_mnemonic(mnemonic)?;
        Ok(())
    }

    async fn load_wallet(&self) -> BackendResult<bool> {
        let mnemonic = match self.read_mnemonic()? {
            Some(m) => m,
            None => return Ok(false),
        };
        let mut wallet = self.wallet.write().await;
        if wallet.wallet_count() > 0 {
            return Ok(true);
        }
        wallet
            .create_wallet_from_mnemonic(
                &mnemonic,
                "",
                0,
                WalletAccountCreationOptions::SpecificAccounts(
                    BTreeSet::from([0]),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    None,
                ),
            )
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        Ok(true)
    }

    fn get_receive_address(&self) -> BackendResult<String> {
        let wallet = self
            .wallet
            .try_read()
            .map_err(|_| BackendError::Internal("wallet lock contention".to_string()))?;

        let wallet_ids: Vec<_> = wallet.list_wallets().into_iter().cloned().collect();
        let wallet_id = wallet_ids.first().ok_or(BackendError::NoWallet)?;
        drop(wallet);

        let mut wallet = self
            .wallet
            .try_write()
            .map_err(|_| BackendError::Internal("wallet lock contention".to_string()))?;

        let result = wallet
            .get_receive_address(wallet_id, 0, AccountTypePreference::PreferBIP44, true)
            .map_err(|e| BackendError::Internal(e.to_string()))?;

        result
            .address
            .map(|a| a.to_string())
            .ok_or_else(|| BackendError::Internal("no address generated".to_string()))
    }

    fn get_balance(&self) -> BackendResult<WalletCoreBalance> {
        let wallet = self
            .wallet
            .try_read()
            .map_err(|_| BackendError::Internal("wallet lock contention".to_string()))?;

        let wallet_ids: Vec<_> = wallet.list_wallets().into_iter().cloned().collect();
        let wallet_id = wallet_ids.first().ok_or(BackendError::NoWallet)?;

        wallet
            .get_wallet_balance(wallet_id)
            .map_err(|e| BackendError::Internal(e.to_string()))
    }

    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>> {
        let wallet = self
            .wallet
            .try_read()
            .map_err(|_| BackendError::Internal("wallet lock contention".to_string()))?;

        let wallet_ids: Vec<_> = wallet.list_wallets().into_iter().cloned().collect();
        let wallet_id = wallet_ids.first().ok_or(BackendError::NoWallet)?;

        let records = wallet
            .wallet_transaction_history(wallet_id)
            .map_err(|e| BackendError::Internal(e.to_string()))?;

        let mut transactions: Vec<TransactionInfo> = records
            .into_iter()
            .map(|r| {
                let direction = if r.net_amount >= 0 {
                    TransactionDirection::Received
                } else {
                    TransactionDirection::Sent
                };
                TransactionInfo {
                    txid: r.txid,
                    amount: r.net_amount,
                    direction,
                    timestamp: r.timestamp,
                    height: r.height,
                    fee: r.fee,
                    addresses: Vec::new(),
                    block_hash: r.block_hash,
                    is_instant_send: false,
                    is_chain_locked: false,
                }
            })
            .collect();

        // Sort: unconfirmed first, then by timestamp descending
        transactions.sort_by(|a, b| match (a.height.is_some(), b.height.is_some()) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => b.timestamp.cmp(&a.timestamp),
        });

        Ok(transactions)
    }

    fn estimate_fee(&self, _address: &str, _amount: u64, fee_rate: u32) -> BackendResult<u64> {
        // Estimate for a typical 1-input 2-output P2PKH transaction (226 bytes)
        let fee = FeeRate::new(fee_rate as u64).calculate_fee(226);
        Ok(fee)
    }

    async fn send(&self, address: &str, amount: u64, fee_rate: u32) -> BackendResult<[u8; 32]> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(BackendError::NotRunning);
        }

        // Parse and validate the recipient address
        let recipient = dashcore::Address::from_str(address)
            .map_err(|e| BackendError::InvalidAddress(e.to_string()))?;
        let recipient = recipient
            .require_network(self.config.network)
            .map_err(|e| BackendError::InvalidAddress(e.to_string()))?;

        // Get wallet data: UTXOs, change address, private keys
        let mut wallet_guard = self.wallet.write().await;
        let wallet_ids: Vec<_> = wallet_guard.list_wallets().into_iter().cloned().collect();
        let wallet_id = wallet_ids.first().ok_or(BackendError::NoWallet)?;

        // Get UTXOs
        let utxos: Vec<_> = wallet_guard
            .wallet_utxos(wallet_id)
            .map_err(|e| BackendError::Internal(e.to_string()))?
            .into_iter()
            .cloned()
            .collect();

        if utxos.is_empty() {
            return Err(BackendError::InsufficientFunds {
                available: 0,
                required: amount,
            });
        }

        // Get a change address
        let change_result = wallet_guard
            .get_change_address(wallet_id, 0, AccountTypePreference::PreferBIP44, true)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let change_address = change_result.address.ok_or_else(|| {
            BackendError::Internal("failed to generate change address".to_string())
        })?;

        // Get wallet reference for key derivation
        let (wallet, info) = wallet_guard
            .get_wallet_and_info(wallet_id)
            .ok_or(BackendError::NoWallet)?;

        let tip_height = self.tip_height().unwrap_or(0);
        let network = self.config.network;
        let accounts = info.accounts();

        // Build the key provider closure that derives private keys for UTXOs
        let key_provider =
            |utxo: &key_wallet_manager::Utxo| -> Option<dashcore::secp256k1::SecretKey> {
                // Search BIP44 accounts first, then BIP32
                for (account_index, account) in &accounts.standard_bip44_accounts {
                    if let ManagedAccountType::Standard {
                        external_addresses,
                        internal_addresses,
                        ..
                    } = &account.account_type
                    {
                        // Check external (receive) addresses
                        if let Some(addr_idx) = external_addresses.address_index(&utxo.address) {
                            let path = DerivationPathBuilder::new()
                                .coin_type(coin_type_for_network(network))
                                .account(*account_index)
                                .change(0)
                                .address_index(addr_idx)
                                .bip44()
                                .ok()?;
                            return wallet.derive_private_key(&path).ok();
                        }
                        // Check internal (change) addresses
                        if let Some(addr_idx) = internal_addresses.address_index(&utxo.address) {
                            let path = DerivationPathBuilder::new()
                                .coin_type(coin_type_for_network(network))
                                .account(*account_index)
                                .change(1)
                                .address_index(addr_idx)
                                .bip44()
                                .ok()?;
                            return wallet.derive_private_key(&path).ok();
                        }
                    }
                }

                for (account_index, account) in &accounts.standard_bip32_accounts {
                    if let ManagedAccountType::Standard {
                        external_addresses,
                        internal_addresses,
                        ..
                    } = &account.account_type
                    {
                        if let Some(addr_idx) = external_addresses.address_index(&utxo.address) {
                            let path = DerivationPathBuilder::new()
                                .account(*account_index)
                                .change(0)
                                .address_index(addr_idx)
                                .build()
                                .ok()?;
                            return wallet.derive_private_key(&path).ok();
                        }
                        if let Some(addr_idx) = internal_addresses.address_index(&utxo.address) {
                            let path = DerivationPathBuilder::new()
                                .account(*account_index)
                                .change(1)
                                .address_index(addr_idx)
                                .build()
                                .ok()?;
                            return wallet.derive_private_key(&path).ok();
                        }
                    }
                }

                None
            };

        // Build the transaction
        let fee = FeeRate::new(fee_rate as u64);
        let mut builder = TransactionBuilder::new()
            .set_fee_rate(fee)
            .set_change_address(change_address)
            .add_output(&recipient, amount)
            .map_err(|e| BackendError::Internal(e.to_string()))?
            .select_inputs(
                &utxos,
                SelectionStrategy::BranchAndBound,
                tip_height,
                key_provider,
            )
            .map_err(|e| match e {
                BuilderError::InsufficientFunds {
                    available,
                    required,
                } => BackendError::InsufficientFunds {
                    available,
                    required,
                },
                BuilderError::CoinSelection(SelectionError::InsufficientFunds {
                    available,
                    required,
                }) => BackendError::InsufficientFunds {
                    available,
                    required,
                },
                other => BackendError::Internal(other.to_string()),
            })?;

        let tx = builder.build().map_err(|e| match e {
            BuilderError::InsufficientFunds {
                available,
                required,
            } => BackendError::InsufficientFunds {
                available,
                required,
            },
            other => BackendError::Internal(other.to_string()),
        })?;

        let txid = tx.txid();

        // Drop the wallet lock before broadcasting
        drop(wallet_guard);

        // Broadcast the transaction
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or(BackendError::NotRunning)?;
        client
            .broadcast_transaction(&tx)
            .await
            .map_err(|e| BackendError::Sync(e.to_string()))?;

        Ok(txid.to_byte_array())
    }

    fn cache_size(&self) -> BackendResult<u64> {
        Ok(super::dir_size_excluding(
            &self.config.data_dir,
            Some(&self.config.wallet_dir()),
        ))
    }

    async fn clear_cache(&self) -> BackendResult<()> {
        if self.is_running() {
            self.stop().await?;
        }

        let data_dir = &self.config.data_dir;
        let wallet_dir = self.config.wallet_dir();

        let entries = std::fs::read_dir(data_dir)
            .map_err(|e| BackendError::Storage(format!("failed to read data dir: {e}")))?;

        for entry in entries.flatten() {
            let path = entry.path();

            // Skip if this path is or contains the wallet directory
            if wallet_dir.starts_with(&path) || path.starts_with(&wallet_dir) {
                continue;
            }

            // Skip config file
            if path.file_name().is_some_and(|n| n == "config.toml") {
                continue;
            }

            if path.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    tracing::warn!("Failed to remove cache dir {}: {e}", path.display());
                }
            } else if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!("Failed to remove cache file {}: {e}", path.display());
            }
        }

        Ok(())
    }

    fn subscribe_events(&self) -> EventReceiver {
        self.event_tx.subscribe()
    }
}

/// Return the BIP44 coin type for the given network.
fn coin_type_for_network(network: Network) -> u32 {
    match network {
        Network::Mainnet => 5,
        _ => 1,
    }
}

async fn event_bridge(
    token: CancellationToken,
    mut progress_rx: tokio::sync::watch::Receiver<SyncProgress>,
    mut sync_rx: broadcast::Receiver<SyncEvent>,
    mut network_rx: broadcast::Receiver<NetworkEvent>,
    mut wallet_rx: broadcast::Receiver<WalletEvent>,
    event_tx: EventSender,
    cached_progress: Arc<RwLock<SyncProgress>>,
) {
    loop {
        tokio::select! {
            Ok(()) = progress_rx.changed() => {
                let p = progress_rx.borrow_and_update().clone();
                if let Ok(mut guard) = cached_progress.write() {
                    *guard = p.clone();
                }
                let _ = event_tx.send(SpvEvent::SyncProgressUpdated(Box::new(p)));
            }
            result = sync_rx.recv() => {
                match result {
                    Ok(event) => {
                        if let Some(e) = map_sync_event(event) {
                            let _ = event_tx.send(e);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Sync events lagged by {n}");
                    }
                    Err(_) => break,
                }
            }
            result = network_rx.recv() => {
                match result {
                    Ok(event) => {
                        let _ = event_tx.send(map_network_event(event));
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Network events lagged by {n}");
                    }
                    Err(_) => break,
                }
            }
            result = wallet_rx.recv() => {
                match result {
                    Ok(event) => {
                        let _ = event_tx.send(map_wallet_event(event));
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Wallet events lagged by {n}");
                    }
                    Err(_) => break,
                }
            }
            _ = token.cancelled() => break,
        }
    }
}

fn map_sync_event(event: SyncEvent) -> Option<SpvEvent> {
    match event {
        SyncEvent::SyncStart { identifier } => Some(SpvEvent::SyncStarted {
            manager: identifier,
        }),
        SyncEvent::BlockHeadersStored { tip_height }
        | SyncEvent::BlockHeaderSyncComplete { tip_height } => {
            Some(SpvEvent::HeadersSynced { tip_height })
        }
        SyncEvent::FiltersSyncComplete { tip_height } => {
            Some(SpvEvent::FiltersSynced { tip_height })
        }
        SyncEvent::BlockProcessed {
            height,
            new_addresses,
            ..
        } => Some(SpvEvent::BlockProcessed {
            height,
            new_addresses: new_addresses.len() as u32,
        }),
        SyncEvent::SyncComplete { header_tip, cycle } => Some(SpvEvent::SyncComplete {
            tip_height: header_tip,
            cycle,
        }),
        SyncEvent::ChainLockReceived {
            chain_lock,
            validated,
        } => Some(SpvEvent::ChainLockReceived {
            height: chain_lock.block_height,
            validated,
        }),
        SyncEvent::InstantLockReceived {
            instant_lock,
            validated,
        } => {
            use dashcore::hashes::Hash;
            Some(SpvEvent::InstantLockReceived {
                txid: instant_lock.txid.to_byte_array(),
                validated,
            })
        }
        SyncEvent::ManagerError { manager, error } => {
            Some(SpvEvent::Error(format!("{manager}: {error}")))
        }
        _ => None,
    }
}

fn map_network_event(event: NetworkEvent) -> SpvEvent {
    match event {
        NetworkEvent::PeerConnected { address } => SpvEvent::PeerConnected(address.to_string()),
        NetworkEvent::PeerDisconnected { address } => {
            SpvEvent::PeerDisconnected(address.to_string())
        }
        NetworkEvent::PeersUpdated {
            connected_count,
            best_height,
            ..
        } => SpvEvent::PeersUpdated {
            count: connected_count as u32,
            best_height: best_height.unwrap_or(0),
        },
    }
}

fn map_wallet_event(event: WalletEvent) -> SpvEvent {
    match event {
        WalletEvent::TransactionReceived {
            txid,
            amount,
            addresses,
            status,
            ..
        } => {
            use dashcore::hashes::Hash;
            let (height, timestamp, block_hash, is_instant_send, is_chain_locked) =
                extract_context_fields(&status);
            SpvEvent::TransactionReceived {
                txid: txid.to_byte_array(),
                amount,
                addresses: addresses.iter().map(|a| a.to_string()).collect(),
                height,
                timestamp,
                block_hash,
                is_instant_send,
                is_chain_locked,
            }
        }
        WalletEvent::TransactionStatusChanged { txid, status, .. } => {
            use dashcore::hashes::Hash;
            let (height, timestamp, block_hash, is_instant_send, is_chain_locked) =
                extract_context_fields(&status);
            SpvEvent::TransactionReceived {
                txid: txid.to_byte_array(),
                amount: 0,
                addresses: Vec::new(),
                height,
                timestamp,
                block_hash,
                is_instant_send,
                is_chain_locked,
            }
        }
        WalletEvent::BalanceUpdated {
            spendable,
            unconfirmed,
            immature,
            locked,
            ..
        } => SpvEvent::BalanceUpdated(WalletCoreBalance::new(
            spendable,
            unconfirmed,
            immature,
            locked,
        )),
    }
}

/// Extract UI-relevant fields from a `TransactionContext`.
fn extract_context_fields(
    ctx: &key_wallet::transaction_checking::TransactionContext,
) -> (Option<u32>, Option<u64>, Option<[u8; 32]>, bool, bool) {
    use dashcore::hashes::Hash;
    use key_wallet::transaction_checking::TransactionContext;
    match ctx {
        TransactionContext::Mempool => (None, None, None, false, false),
        TransactionContext::InstantSend => (None, None, None, true, false),
        TransactionContext::InBlock {
            height,
            timestamp,
            block_hash,
        } => (
            Some(*height),
            timestamp.map(|t| t as u64),
            block_hash.map(|h| h.to_byte_array()),
            false,
            false,
        ),
        TransactionContext::InChainLockedBlock {
            height,
            timestamp,
            block_hash,
        } => (
            Some(*height),
            timestamp.map(|t| t as u64),
            block_hash.map(|h| h.to_byte_array()),
            false,
            true,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use dash_spv::network::NetworkEvent;
    use dash_spv::sync::{ManagerIdentifier, SyncEvent};
    use dashcore::bls_sig_utils::BLSSignature;
    use dashcore::ephemerealdata::chain_lock::ChainLock;
    use dashcore::ephemerealdata::instant_lock::InstantLock;
    use dashcore::hashes::Hash;
    use dashcore::{Address, BlockHash, PublicKey, Txid};
    use key_wallet::transaction_checking::TransactionContext;
    use key_wallet_manager::WalletEvent;

    use super::*;

    fn test_address() -> Address {
        let pk = PublicKey::from_slice(&[
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x01,
        ])
        .unwrap();
        Address::p2pkh(&pk, Network::Testnet)
    }

    #[test]
    fn coin_type_mainnet_returns_5() {
        assert_eq!(coin_type_for_network(Network::Mainnet), 5);
    }

    #[test]
    fn coin_type_non_mainnet_returns_1() {
        assert_eq!(coin_type_for_network(Network::Testnet), 1);
        assert_eq!(coin_type_for_network(Network::Regtest), 1);
        assert_eq!(coin_type_for_network(Network::Devnet), 1);
    }

    #[test]
    fn extract_context_mempool() {
        let (height, timestamp, block_hash, is, cl) =
            extract_context_fields(&TransactionContext::Mempool);
        assert_eq!(height, None);
        assert_eq!(timestamp, None);
        assert_eq!(block_hash, None);
        assert!(!is);
        assert!(!cl);
    }

    #[test]
    fn extract_context_instant_send() {
        let (height, timestamp, block_hash, is, cl) =
            extract_context_fields(&TransactionContext::InstantSend);
        assert_eq!(height, None);
        assert_eq!(timestamp, None);
        assert_eq!(block_hash, None);
        assert!(is);
        assert!(!cl);
    }

    #[test]
    fn extract_context_in_block() {
        let hash = BlockHash::all_zeros();
        let ctx = TransactionContext::InBlock {
            height: 1000,
            timestamp: Some(1700000000),
            block_hash: Some(hash),
        };
        let (height, timestamp, block_hash, is, cl) = extract_context_fields(&ctx);
        assert_eq!(height, Some(1000));
        assert_eq!(timestamp, Some(1700000000));
        assert_eq!(block_hash, Some(hash.to_byte_array()));
        assert!(!is);
        assert!(!cl);
    }

    #[test]
    fn extract_context_in_block_none_optionals() {
        let ctx = TransactionContext::InBlock {
            height: 500,
            timestamp: None,
            block_hash: None,
        };
        let (height, timestamp, block_hash, is, cl) = extract_context_fields(&ctx);
        assert_eq!(height, Some(500));
        assert_eq!(timestamp, None);
        assert_eq!(block_hash, None);
        assert!(!is);
        assert!(!cl);
    }

    #[test]
    fn extract_context_in_chain_locked_block() {
        let hash = BlockHash::all_zeros();
        let ctx = TransactionContext::InChainLockedBlock {
            height: 2000,
            timestamp: Some(1700001000),
            block_hash: Some(hash),
        };
        let (height, timestamp, block_hash, is, cl) = extract_context_fields(&ctx);
        assert_eq!(height, Some(2000));
        assert_eq!(timestamp, Some(1700001000));
        assert_eq!(block_hash, Some(hash.to_byte_array()));
        assert!(!is);
        assert!(cl);
    }

    #[test]
    fn map_sync_event_sync_start() {
        let event = SyncEvent::SyncStart {
            identifier: ManagerIdentifier::BlockHeader,
        };
        let mapped = map_sync_event(event);
        assert_eq!(
            mapped,
            Some(SpvEvent::SyncStarted {
                manager: ManagerIdentifier::BlockHeader,
            })
        );
    }

    #[test]
    fn map_sync_event_block_headers_stored() {
        let event = SyncEvent::BlockHeadersStored { tip_height: 5000 };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::HeadersSynced { tip_height: 5000 })
        );
    }

    #[test]
    fn map_sync_event_block_header_sync_complete() {
        let event = SyncEvent::BlockHeaderSyncComplete { tip_height: 5000 };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::HeadersSynced { tip_height: 5000 })
        );
    }

    #[test]
    fn map_sync_event_filters_sync_complete() {
        let event = SyncEvent::FiltersSyncComplete { tip_height: 4000 };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::FiltersSynced { tip_height: 4000 })
        );
    }

    #[test]
    fn map_sync_event_block_processed() {
        let event = SyncEvent::BlockProcessed {
            block_hash: BlockHash::all_zeros(),
            height: 100,
            new_addresses: vec![test_address()],
            confirmed_txids: vec![],
        };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::BlockProcessed {
                height: 100,
                new_addresses: 1,
            })
        );
    }

    #[test]
    fn map_sync_event_sync_complete() {
        let event = SyncEvent::SyncComplete {
            header_tip: 6000,
            cycle: 1,
        };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::SyncComplete {
                tip_height: 6000,
                cycle: 1,
            })
        );
    }

    #[test]
    fn map_sync_event_chain_lock_received() {
        let chain_lock = ChainLock {
            block_height: 7777,
            block_hash: BlockHash::all_zeros(),
            signature: BLSSignature::from([0; 96]),
        };
        let event = SyncEvent::ChainLockReceived {
            chain_lock,
            validated: true,
        };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::ChainLockReceived {
                height: 7777,
                validated: true,
            })
        );
    }

    #[test]
    fn map_sync_event_instant_lock_received() {
        let instant_lock = InstantLock {
            txid: Txid::all_zeros(),
            ..InstantLock::default()
        };
        let event = SyncEvent::InstantLockReceived {
            instant_lock,
            validated: false,
        };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::InstantLockReceived {
                txid: Txid::all_zeros().to_byte_array(),
                validated: false,
            })
        );
    }

    #[test]
    fn map_sync_event_manager_error() {
        let event = SyncEvent::ManagerError {
            manager: ManagerIdentifier::Filter,
            error: "timeout".to_string(),
        };
        assert_eq!(
            map_sync_event(event),
            Some(SpvEvent::Error("Filter: timeout".to_string()))
        );
    }

    #[test]
    fn map_sync_event_unmapped_returns_none() {
        let unmapped = vec![
            SyncEvent::FilterHeadersStored {
                start_height: 0,
                end_height: 100,
                tip_height: 100,
            },
            SyncEvent::FilterHeadersSyncComplete { tip_height: 100 },
            SyncEvent::FiltersStored {
                start_height: 0,
                end_height: 100,
            },
            SyncEvent::BlocksNeeded {
                blocks: Default::default(),
            },
            SyncEvent::MasternodeStateUpdated { height: 100 },
        ];
        for event in unmapped {
            assert_eq!(map_sync_event(event), None);
        }
    }

    #[test]
    fn map_network_event_peer_connected() {
        let addr: SocketAddr = "192.168.1.1:9999".parse().unwrap();
        let event = NetworkEvent::PeerConnected { address: addr };
        assert_eq!(
            map_network_event(event),
            SpvEvent::PeerConnected("192.168.1.1:9999".to_string())
        );
    }

    #[test]
    fn map_network_event_peer_disconnected() {
        let addr: SocketAddr = "10.0.0.1:19999".parse().unwrap();
        let event = NetworkEvent::PeerDisconnected { address: addr };
        assert_eq!(
            map_network_event(event),
            SpvEvent::PeerDisconnected("10.0.0.1:19999".to_string())
        );
    }

    #[test]
    fn map_network_event_peers_updated() {
        let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let event = NetworkEvent::PeersUpdated {
            connected_count: 3,
            addresses: vec![addr],
            best_height: Some(10000),
        };
        assert_eq!(
            map_network_event(event),
            SpvEvent::PeersUpdated {
                count: 3,
                best_height: 10000,
            }
        );
    }

    #[test]
    fn map_network_event_peers_updated_no_best_height() {
        let event = NetworkEvent::PeersUpdated {
            connected_count: 0,
            addresses: vec![],
            best_height: None,
        };
        assert_eq!(
            map_network_event(event),
            SpvEvent::PeersUpdated {
                count: 0,
                best_height: 0,
            }
        );
    }

    #[test]
    fn map_wallet_event_transaction_received() {
        let txid = Txid::all_zeros();
        let event = WalletEvent::TransactionReceived {
            wallet_id: [0; 32],
            status: TransactionContext::Mempool,
            account_index: 0,
            txid,
            amount: 50000,
            addresses: vec![test_address()],
        };
        let mapped = map_wallet_event(event);
        match mapped {
            SpvEvent::TransactionReceived {
                txid: mapped_txid,
                amount,
                addresses,
                height,
                is_instant_send,
                is_chain_locked,
                ..
            } => {
                assert_eq!(mapped_txid, txid.to_byte_array());
                assert_eq!(amount, 50000);
                assert_eq!(addresses.len(), 1);
                assert_eq!(height, None);
                assert!(!is_instant_send);
                assert!(!is_chain_locked);
            }
            other => panic!("expected TransactionReceived, got {:?}", other),
        }
    }

    #[test]
    fn map_wallet_event_transaction_status_changed() {
        let txid = Txid::all_zeros();
        let event = WalletEvent::TransactionStatusChanged {
            wallet_id: [0; 32],
            txid,
            status: TransactionContext::InBlock {
                height: 300,
                timestamp: Some(1700000000),
                block_hash: Some(BlockHash::all_zeros()),
            },
        };
        let mapped = map_wallet_event(event);
        match mapped {
            SpvEvent::TransactionReceived {
                amount,
                height,
                timestamp,
                is_chain_locked,
                ..
            } => {
                assert_eq!(amount, 0);
                assert_eq!(height, Some(300));
                assert_eq!(timestamp, Some(1700000000));
                assert!(!is_chain_locked);
            }
            other => panic!("expected TransactionReceived, got {:?}", other),
        }
    }

    #[test]
    fn map_wallet_event_balance_updated() {
        let event = WalletEvent::BalanceUpdated {
            wallet_id: [0; 32],
            spendable: 100_000,
            unconfirmed: 50_000,
            immature: 25_000,
            locked: 10_000,
        };
        let mapped = map_wallet_event(event);
        assert_eq!(
            mapped,
            SpvEvent::BalanceUpdated(WalletCoreBalance::new(100_000, 50_000, 25_000, 10_000))
        );
    }
}
