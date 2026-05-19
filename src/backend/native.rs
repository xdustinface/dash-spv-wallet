use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use dash_spv::client::EventHandler;
use dash_spv::network::NetworkEvent;
use dash_spv::network::manager::PeerNetworkManager;
use dash_spv::storage::DiskStorageManager;
use dash_spv::sync::SyncEvent;
use dash_spv::{ClientConfig, DashSpvClient, MempoolStrategy};
use dashcore::hashes::Hash;
use key_wallet::Mnemonic;
use key_wallet::managed_account::managed_account_trait::ManagedAccountTrait;
use key_wallet::managed_account::transaction_record::{
    OutputRole as UpstreamOutputRole, TransactionRecord,
};
use key_wallet::mnemonic::Language;
use key_wallet::transaction_checking::TransactionContext;
use key_wallet::wallet::initialization::WalletAccountCreationOptions;
use key_wallet::wallet::managed_wallet_info::ManagedWalletInfo;
use key_wallet::wallet::managed_wallet_info::coin_selection::SelectionError;
use key_wallet::wallet::managed_wallet_info::coin_selection::SelectionStrategy;
use key_wallet::wallet::managed_wallet_info::fee::FeeRate;
use key_wallet::wallet::managed_wallet_info::transaction_builder::BuilderError;
use key_wallet::wallet::managed_wallet_info::transaction_builder::TransactionBuilder;
use key_wallet::wallet::managed_wallet_info::transaction_building::AccountTypePreference;
use key_wallet::wallet::managed_wallet_info::wallet_info_interface::WalletInfoInterface;
use key_wallet_manager::{WalletEvent, WalletManager};
use tokio_util::sync::CancellationToken;

use super::error::{BackendError, BackendResult};
use super::events::{EventReceiver, EventSender, SpvEvent, event_channel};
use super::r#trait::SpvBackend;
use super::types::{
    InputInfo, Network, OutputInfo, OutputRole, SyncProgress, TransactionDirection,
    TransactionInfo, TransactionType, WalletCoreBalance,
};
use crate::config::AppConfig;

type SpvClient =
    DashSpvClient<WalletManager<ManagedWalletInfo>, PeerNetworkManager, DiskStorageManager>;

/// Implements `EventHandler` to bridge SPV client events into the UI event channel.
struct NativeEventHandler {
    event_tx: EventSender,
    progress: Arc<RwLock<SyncProgress>>,
    network: Network,
}

impl EventHandler for NativeEventHandler {
    fn on_sync_event(&self, event: &SyncEvent) {
        if let Some(e) = map_sync_event(event.clone()) {
            let _ = self.event_tx.send(e);
        }
    }

    fn on_network_event(&self, event: &NetworkEvent) {
        let _ = self.event_tx.send(map_network_event(event.clone()));
    }

    fn on_progress(&self, progress: &SyncProgress) {
        if let Ok(mut guard) = self.progress.write() {
            *guard = progress.clone();
        }
        let _ = self
            .event_tx
            .send(SpvEvent::SyncProgressUpdated(Box::new(progress.clone())));
    }

    fn on_wallet_event(&self, event: &WalletEvent) {
        for spv_event in map_wallet_event(event.clone(), self.network) {
            let _ = self.event_tx.send(spv_event);
        }
    }

    fn on_error(&self, error: &str) {
        let _ = self.event_tx.send(SpvEvent::Error(error.to_string()));
    }
}

pub struct NativeBackend {
    config: AppConfig,
    wallet: Arc<tokio::sync::RwLock<WalletManager<ManagedWalletInfo>>>,
    running: AtomicBool,
    event_tx: EventSender,
    progress: Arc<RwLock<SyncProgress>>,
    shutdown_token: tokio::sync::Mutex<Option<CancellationToken>>,
    client_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
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
            .with_storage_path(self.config.network_data_dir())
            .with_user_agent("dash-spv-ui");

        if self.config.network == Network::Regtest {
            client_config = client_config.without_masternodes();
        }

        if !self.config.peers().is_empty() {
            client_config.peers.clear();
            for peer in self.config.peers() {
                if let Ok(addr) = peer.parse::<SocketAddr>() {
                    client_config.add_peer(addr);
                }
            }
            client_config = client_config.with_restrict_to_configured_peers(true);
        }

        let mempool_strategy = match self.config.mempool_strategy() {
            "fetch-all" => MempoolStrategy::FetchAll,
            _ => MempoolStrategy::BloomFilter,
        };
        client_config = client_config.with_mempool_tracking(mempool_strategy);

        let network_manager = PeerNetworkManager::new(&client_config)
            .await
            .map_err(|e| BackendError::Sync(e.to_string()))?;

        let storage_manager = DiskStorageManager::new(&client_config)
            .await
            .map_err(|e| BackendError::Storage(e.to_string()))?;

        let event_handler = Arc::new(NativeEventHandler {
            event_tx: self.event_tx.clone(),
            progress: self.progress.clone(),
            network: self.config.network,
        });

        let client = Arc::new(
            SpvClient::new(
                client_config,
                network_manager,
                storage_manager,
                self.wallet.clone(),
                vec![event_handler],
            )
            .await
            .map_err(|e| BackendError::Internal(e.to_string()))?,
        );

        let token = CancellationToken::new();

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

        wallet
            .next_receive_address(wallet_id, 0, AccountTypePreference::BIP44, true)
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
            .map(|r| TransactionInfo::from_record(r, 0, self.config.network))
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
        let change_address = wallet_guard
            .next_change_address(wallet_id, 0, AccountTypePreference::BIP44, true)
            .ok_or_else(|| {
                BackendError::Internal("failed to generate change address".to_string())
            })?;

        // Get wallet reference for signing and the managed accounts for path resolution
        let (wallet, info) = wallet_guard
            .get_wallet_and_info(wallet_id)
            .ok_or(BackendError::NoWallet)?;

        let tip_height = self.tip_height().unwrap_or(0);
        let accounts = info.accounts();

        let path_resolver = |address: dashcore::Address| -> Option<key_wallet::DerivationPath> {
            accounts
                .standard_bip44_accounts
                .values()
                .chain(accounts.standard_bip32_accounts.values())
                .find_map(|account| account.address_derivation_path(&address))
        };

        let fee = FeeRate::new(fee_rate as u64);
        let (tx, actual_fee) = TransactionBuilder::new()
            .set_fee_rate(fee)
            .set_change_address(change_address)
            .add_output(&recipient, amount)
            .set_selection_strategy(SelectionStrategy::BranchAndBound)
            .set_current_height(tip_height)
            .add_inputs(utxos)
            .build_signed(wallet, path_resolver)
            .await
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

        let txid = tx.txid();
        tracing::debug!(txid = %txid, actual_fee, "transaction built");

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
        Ok(super::dir_size(&self.config.network_data_dir()))
    }

    async fn clear_cache(&self) -> BackendResult<()> {
        if self.is_running() {
            self.stop().await?;
        }

        self.config
            .clear_network_data_dir()
            .map_err(|e| BackendError::Storage(e.to_string()))
    }

    fn subscribe_events(&self) -> EventReceiver {
        self.event_tx.subscribe()
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
        } => Some(SpvEvent::InstantLockReceived {
            txid: instant_lock.txid.to_byte_array(),
            validated,
        }),
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

fn map_wallet_event(event: WalletEvent, network: Network) -> Vec<SpvEvent> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match event {
        WalletEvent::TransactionDetected {
            record, balance, ..
        } => vec![
            SpvEvent::TransactionReceived(Box::new(TransactionInfo::from_record(
                &record, now, network,
            ))),
            SpvEvent::BalanceUpdated(balance),
        ],
        WalletEvent::TransactionInstantLocked { txid, balance, .. } => {
            vec![
                SpvEvent::TransactionReceived(Box::new(TransactionInfo {
                    txid,
                    is_instant_send: true,
                    amount: 0,
                    direction: TransactionDirection::Incoming,
                    transaction_type: TransactionType::Standard,
                    timestamp: 0,
                    height: None,
                    fee: None,
                    addresses: Vec::new(),
                    block_hash: None,
                    is_chain_locked: false,
                    label: None,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                })),
                SpvEvent::BalanceUpdated(balance),
            ]
        }
        WalletEvent::BlockProcessed {
            inserted,
            updated,
            matured,
            balance,
            ..
        } => {
            let mut events = Vec::with_capacity(inserted.len() + updated.len() + matured.len() + 1);
            for record in inserted.iter().chain(updated.iter()).chain(matured.iter()) {
                events.push(SpvEvent::TransactionReceived(Box::new(
                    TransactionInfo::from_record(record, now, network),
                )));
            }
            events.push(SpvEvent::BalanceUpdated(balance));
            events
        }
        WalletEvent::SyncHeightAdvanced { .. } => Vec::new(),
        WalletEvent::TransactionsChainlocked { .. } => Vec::new(),
    }
}

impl TransactionInfo {
    /// Build a `TransactionInfo` from a wallet `TransactionRecord`.
    ///
    /// `fallback_timestamp` is used when the transaction context carries no
    /// timestamp (mempool / instant-send).  Pass `0` for cold-start loads
    /// (renders as "Pending") or the current unix time for live events
    /// (renders as "just now").
    fn from_record(record: &TransactionRecord, fallback_timestamp: u64, network: Network) -> Self {
        let (height, timestamp, block_hash, is_instant_send, is_chain_locked) =
            extract_context_fields(&record.context);
        let addresses = extract_record_addresses(record);
        let inputs = extract_record_inputs(record);
        let outputs = extract_record_outputs(record, network);
        TransactionInfo {
            txid: record.txid,
            amount: record.net_amount,
            direction: record.direction,
            transaction_type: record.transaction_type,
            timestamp: timestamp.unwrap_or(fallback_timestamp),
            height,
            fee: record.fee,
            addresses,
            block_hash: block_hash.map(dashcore::BlockHash::from_byte_array),
            is_instant_send,
            is_chain_locked,
            label: if record.label.is_empty() {
                None
            } else {
                Some(record.label.clone())
            },
            inputs,
            outputs,
        }
    }
}

/// Extract unique addresses from a transaction record's input details.
fn extract_record_addresses(record: &TransactionRecord) -> Vec<String> {
    let mut addrs: Vec<String> = record
        .input_details
        .iter()
        .map(|d| d.address.to_string())
        .collect();
    addrs.sort();
    addrs.dedup();
    addrs
}

/// Build `InputInfo` entries from a transaction record's input details.
fn extract_record_inputs(record: &TransactionRecord) -> Vec<InputInfo> {
    record
        .input_details
        .iter()
        .map(|d| InputInfo {
            index: d.index,
            value: d.value,
            address: d.address.to_string(),
        })
        .collect()
}

/// Build `OutputInfo` entries from a transaction record's output details,
/// enriching them with value and address from the raw transaction outputs.
fn extract_record_outputs(record: &TransactionRecord, network: Network) -> Vec<OutputInfo> {
    record
        .output_details
        .iter()
        .map(|d| {
            let tx_out = record.transaction.output.get(d.index as usize);
            if tx_out.is_none() {
                tracing::warn!(
                    requested_index = d.index,
                    actual_len = record.transaction.output.len(),
                    "native output index out of bounds, returning zero value and empty address"
                );
            }
            let value = tx_out.map_or(0, |o| o.value);
            let address = tx_out
                .and_then(|o| dashcore::Address::from_script(&o.script_pubkey, network).ok())
                .map_or_else(String::new, |a| a.to_string());
            OutputInfo {
                index: d.index,
                value,
                address,
                role: map_output_role(d.role),
            }
        })
        .collect()
}

fn map_output_role(role: UpstreamOutputRole) -> OutputRole {
    match role {
        UpstreamOutputRole::Received => OutputRole::Received,
        UpstreamOutputRole::Change => OutputRole::Change,
        UpstreamOutputRole::Sent => OutputRole::Sent,
        UpstreamOutputRole::Unspendable => OutputRole::Unspendable,
    }
}

/// Extract UI-relevant fields from a `TransactionContext`.
fn extract_context_fields(
    ctx: &TransactionContext,
) -> (Option<u32>, Option<u64>, Option<[u8; 32]>, bool, bool) {
    match ctx {
        TransactionContext::Mempool => (None, None, None, false, false),
        TransactionContext::InstantSend(_) => (None, None, None, true, false),
        TransactionContext::InBlock(info) => (
            Some(info.height()),
            Some(info.timestamp() as u64),
            Some(info.block_hash().to_byte_array()),
            false,
            false,
        ),
        TransactionContext::InChainLockedBlock(info) => (
            Some(info.height()),
            Some(info.timestamp() as u64),
            Some(info.block_hash().to_byte_array()),
            false,
            true,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::net::SocketAddr;

    use dash_spv::network::NetworkEvent;
    use dash_spv::sync::{ManagerIdentifier, SyncEvent};
    use dashcore::blockdata::transaction::Transaction;
    use dashcore::bls_sig_utils::BLSSignature;
    use dashcore::ephemerealdata::chain_lock::ChainLock;
    use dashcore::ephemerealdata::instant_lock::InstantLock;
    use dashcore::hashes::Hash;
    use dashcore::{Address, BlockHash, PublicKey, Txid};
    use key_wallet::account::{AccountType, StandardAccountType};
    use key_wallet::managed_account::transaction_record::{
        InputDetail, OutputDetail, OutputRole as UpstreamTestOutputRole, TransactionRecord,
    };
    use key_wallet::transaction_checking::TransactionContext;
    use key_wallet::transaction_checking::transaction_context::BlockInfo;
    use key_wallet::transaction_checking::transaction_router::TransactionType;
    use key_wallet_manager::WalletEvent;

    use super::*;
    use crate::backend::types::TransactionDirection;

    fn standard_account_type() -> AccountType {
        AccountType::Standard {
            index: 0,
            standard_account_type: StandardAccountType::BIP44Account,
        }
    }

    fn make_record(
        tx: dashcore::Transaction,
        context: TransactionContext,
        input_details: Vec<InputDetail>,
        output_details: Vec<OutputDetail>,
        net_amount: i64,
    ) -> TransactionRecord {
        TransactionRecord::new(
            tx,
            standard_account_type(),
            context,
            TransactionType::Standard,
            TransactionDirection::Incoming,
            input_details,
            output_details,
            net_amount,
        )
    }

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
        let is_lock = InstantLock::default();
        let (height, timestamp, block_hash, is, cl) =
            extract_context_fields(&TransactionContext::InstantSend(is_lock));
        assert_eq!(height, None);
        assert_eq!(timestamp, None);
        assert_eq!(block_hash, None);
        assert!(is);
        assert!(!cl);
    }

    #[test]
    fn extract_context_in_block() {
        let hash = BlockHash::all_zeros();
        let ctx = TransactionContext::InBlock(BlockInfo::new(1000, hash, 1700000000));
        let (height, timestamp, block_hash, is, cl) = extract_context_fields(&ctx);
        assert_eq!(height, Some(1000));
        assert_eq!(timestamp, Some(1700000000));
        assert_eq!(block_hash, Some(hash.to_byte_array()));
        assert!(!is);
        assert!(!cl);
    }

    #[test]
    fn extract_context_in_chain_locked_block() {
        let hash = BlockHash::all_zeros();
        let ctx = TransactionContext::InChainLockedBlock(BlockInfo::new(2000, hash, 1700001000));
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
        let mut new_addresses = BTreeMap::new();
        new_addresses.insert([0u8; 32], vec![test_address()]);
        let event = SyncEvent::BlockProcessed {
            block_hash: BlockHash::all_zeros(),
            height: 100,
            wallets: BTreeSet::new(),
            new_addresses,
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
            SyncEvent::MasternodeStateUpdated {
                height: 100,
                qr_info_result: None,
            },
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
    fn map_wallet_event_transaction_detected() {
        let tx = Transaction::dummy_empty();
        let txid = tx.txid();
        let record = make_record(
            tx,
            TransactionContext::Mempool,
            Vec::new(),
            Vec::new(),
            50000,
        );
        let event = WalletEvent::TransactionDetected {
            wallet_id: [0; 32],
            record: Box::new(record),
            balance: WalletCoreBalance::new(50000, 0, 0, 0),
            account_balances: Default::default(),
            addresses_derived: Vec::new(),
        };
        let mapped = map_wallet_event(event, Network::Mainnet);
        assert_eq!(mapped.len(), 2);
        match &mapped[0] {
            SpvEvent::TransactionReceived(info) => {
                assert_eq!(info.txid, txid);
                assert_eq!(info.amount, 50000);
                assert_eq!(info.direction, TransactionDirection::Incoming);
                assert!(info.addresses.is_empty());
                assert_eq!(info.height, None);
                assert!(!info.is_instant_send);
                assert!(!info.is_chain_locked);
            }
            other => panic!("expected TransactionReceived, got {:?}", other),
        }
        assert_eq!(
            mapped[1],
            SpvEvent::BalanceUpdated(WalletCoreBalance::new(50000, 0, 0, 0))
        );
    }

    #[test]
    fn map_wallet_event_block_processed_emits_balance() {
        let event = WalletEvent::BlockProcessed {
            wallet_id: [0; 32],
            height: 100,
            chain_lock: None,
            inserted: Vec::new(),
            updated: Vec::new(),
            matured: Vec::new(),
            balance: WalletCoreBalance::new(100_000, 50_000, 25_000, 10_000),
            account_balances: Default::default(),
            addresses_derived: Vec::new(),
        };
        let mapped = map_wallet_event(event, Network::Mainnet);
        assert_eq!(mapped.len(), 1);
        assert_eq!(
            mapped[0],
            SpvEvent::BalanceUpdated(WalletCoreBalance::new(100_000, 50_000, 25_000, 10_000))
        );
    }

    #[test]
    fn map_wallet_event_block_processed_with_records_emits_transaction_received() {
        let tx1 = Transaction::dummy_empty();
        let tx2 = Transaction::dummy_empty();
        let txid1 = tx1.txid();
        let txid2 = tx2.txid();
        let record1 = make_record(
            tx1,
            TransactionContext::Mempool,
            Vec::new(),
            Vec::new(),
            10_000,
        );
        let record2 = make_record(
            tx2,
            TransactionContext::Mempool,
            Vec::new(),
            Vec::new(),
            20_000,
        );
        let balance = WalletCoreBalance::new(30_000, 0, 0, 0);
        let event = WalletEvent::BlockProcessed {
            wallet_id: [0; 32],
            height: 200,
            chain_lock: None,
            inserted: vec![record1],
            updated: vec![record2],
            matured: Vec::new(),
            balance,
            account_balances: Default::default(),
            addresses_derived: Vec::new(),
        };
        let mapped = map_wallet_event(event, Network::Mainnet);
        assert_eq!(mapped.len(), 3);
        match &mapped[0] {
            SpvEvent::TransactionReceived(info) => assert_eq!(info.txid, txid1),
            other => panic!("expected TransactionReceived, got {:?}", other),
        }
        match &mapped[1] {
            SpvEvent::TransactionReceived(info) => assert_eq!(info.txid, txid2),
            other => panic!("expected TransactionReceived, got {:?}", other),
        }
        assert_eq!(mapped[2], SpvEvent::BalanceUpdated(balance));
    }

    #[test]
    fn map_wallet_event_transaction_instant_locked_emits_received_and_balance() {
        let txid = Txid::from_byte_array([7; 32]);
        let balance = WalletCoreBalance::new(200_000, 0, 0, 0);
        let event = WalletEvent::TransactionInstantLocked {
            wallet_id: [0; 32],
            txid,
            instant_lock: InstantLock::default(),
            balance,
            account_balances: Default::default(),
        };
        let mapped = map_wallet_event(event, Network::Mainnet);
        assert_eq!(mapped.len(), 2);
        match &mapped[0] {
            SpvEvent::TransactionReceived(info) => {
                assert_eq!(info.txid, txid);
                assert!(info.is_instant_send);
                assert_eq!(info.amount, 0);
            }
            other => panic!("expected TransactionReceived, got {:?}", other),
        }
        assert_eq!(mapped[1], SpvEvent::BalanceUpdated(balance));
    }

    #[test]
    fn map_wallet_event_sync_height_advanced_is_empty() {
        let event = WalletEvent::SyncHeightAdvanced {
            wallet_id: [0; 32],
            height: 200,
        };
        let mapped = map_wallet_event(event, Network::Mainnet);
        assert!(mapped.is_empty());
    }

    #[test]
    fn from_record_in_block_uses_block_timestamp_over_fallback() {
        let tx = Transaction::dummy_empty();
        let block_hash = BlockHash::all_zeros();
        let record = make_record(
            tx,
            TransactionContext::InBlock(BlockInfo::new(500, block_hash, 1700000000)),
            Vec::new(),
            Vec::new(),
            42000,
        );

        let info = TransactionInfo::from_record(&record, 9999999999, Network::Mainnet);

        assert_eq!(info.timestamp, 1700000000);
        assert_eq!(info.height, Some(500));
        assert_eq!(info.direction, TransactionDirection::Incoming);
    }

    #[test]
    fn from_record_mempool_uses_fallback_timestamp() {
        let tx = Transaction::dummy_empty();
        let record = make_record(
            tx,
            TransactionContext::Mempool,
            Vec::new(),
            Vec::new(),
            10000,
        );

        let info = TransactionInfo::from_record(&record, 0, Network::Mainnet);

        assert_eq!(info.timestamp, 0);
        assert_eq!(info.height, None);
    }

    #[test]
    fn from_record_non_empty_label_is_preserved() {
        let tx = Transaction::dummy_empty();
        let mut record = make_record(
            tx,
            TransactionContext::Mempool,
            Vec::new(),
            Vec::new(),
            1_000,
        );
        record.label = "grocery store".to_string();
        let info = TransactionInfo::from_record(&record, 0, Network::Testnet);
        assert_eq!(info.label, Some("grocery store".to_string()));
    }

    #[test]
    fn from_record_empty_label_maps_to_none() {
        let tx = Transaction::dummy_empty();
        let record = make_record(
            tx,
            TransactionContext::Mempool,
            Vec::new(),
            Vec::new(),
            1_000,
        );
        let info = TransactionInfo::from_record(&record, 0, Network::Testnet);
        assert_eq!(info.label, None);
    }

    #[test]
    fn extract_record_addresses_deduplicates() {
        let addr_a = test_address();
        // Create a distinct address using a different public key
        let pk_b = PublicKey::from_slice(&[
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x02,
        ])
        .unwrap();
        let addr_b = Address::p2pkh(&pk_b, Network::Testnet);

        let input_details = vec![
            InputDetail {
                index: 0,
                value: 1000,
                address: addr_a.clone(),
            },
            InputDetail {
                index: 1,
                value: 2000,
                address: addr_a.clone(),
            },
            InputDetail {
                index: 2,
                value: 3000,
                address: addr_b.clone(),
            },
            InputDetail {
                index: 3,
                value: 4000,
                address: addr_b.clone(),
            },
        ];

        let tx = Transaction::dummy_empty();
        let record = make_record(
            tx,
            TransactionContext::Mempool,
            input_details,
            Vec::new(),
            10000,
        );

        let addresses = extract_record_addresses(&record);
        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&addr_a.to_string()));
        assert!(addresses.contains(&addr_b.to_string()));
    }

    #[test]
    fn extract_record_inputs_maps_index_value_address() {
        let addr = test_address();
        let input_details = vec![
            InputDetail {
                index: 0,
                value: 50_000,
                address: addr.clone(),
            },
            InputDetail {
                index: 1,
                value: 75_000,
                address: addr.clone(),
            },
        ];

        let tx = Transaction::dummy_empty();
        let record = make_record(
            tx,
            TransactionContext::Mempool,
            input_details,
            Vec::new(),
            125_000,
        );

        let inputs = extract_record_inputs(&record);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].index, 0);
        assert_eq!(inputs[0].value, 50_000);
        assert_eq!(inputs[0].address, addr.to_string());
        assert_eq!(inputs[1].index, 1);
        assert_eq!(inputs[1].value, 75_000);
    }

    #[test]
    fn extract_record_outputs_enriches_value_and_address_from_tx() {
        let addr = test_address();
        let tx = Transaction::dummy(&addr, 0..1, &[99_774, 226]);
        let output_details = vec![
            OutputDetail {
                index: 0,
                role: UpstreamTestOutputRole::Received,
                address: Some(addr.clone()),
                value: 99_774,
            },
            OutputDetail {
                index: 1,
                role: UpstreamTestOutputRole::Change,
                address: Some(addr.clone()),
                value: 226,
            },
        ];

        let record = make_record(
            tx,
            TransactionContext::Mempool,
            Vec::new(),
            output_details,
            99_774,
        );

        let outputs = extract_record_outputs(&record, Network::Testnet);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].index, 0);
        assert_eq!(outputs[0].value, 99_774);
        assert_eq!(outputs[0].address, addr.to_string());
        assert_eq!(outputs[0].role, OutputRole::Received);
        assert_eq!(outputs[1].index, 1);
        assert_eq!(outputs[1].value, 226);
        assert_eq!(outputs[1].role, OutputRole::Change);
    }

    #[test]
    fn extract_record_outputs_oob_index_falls_back_to_zero() {
        let addr = test_address();
        let tx = Transaction::dummy(&addr, 0..1, &[50_000]);
        // index 99 is out of bounds for a 1-output tx
        let output_details = vec![OutputDetail {
            index: 99,
            role: UpstreamTestOutputRole::Sent,
            address: None,
            value: 0,
        }];

        let record = make_record(
            tx,
            TransactionContext::Mempool,
            Vec::new(),
            output_details,
            0,
        );

        let outputs = extract_record_outputs(&record, Network::Testnet);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].index, 99);
        assert_eq!(outputs[0].value, 0);
        assert_eq!(outputs[0].address, "");
        assert_eq!(outputs[0].role, OutputRole::Sent);
    }
}
