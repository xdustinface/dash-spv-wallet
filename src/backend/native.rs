use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use dash_spv::network::manager::PeerNetworkManager;
use dash_spv::storage::DiskStorageManager;
use dash_spv::sync::SyncEvent;
use dash_spv::network::NetworkEvent;
use dash_spv::{ClientConfig, DashSpvClient};
use key_wallet::manager::{WalletEvent, WalletManager};
use key_wallet::wallet::initialization::WalletAccountCreationOptions;
use key_wallet::wallet::managed_wallet_info::transaction_building::AccountTypePreference;
use key_wallet::wallet::managed_wallet_info::ManagedWalletInfo;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::error::{BackendError, BackendResult};
use super::events::{event_channel, EventReceiver, EventSender, SpvEvent};
use super::r#trait::SpvBackend;
use super::types::{
    Network, SyncProgress, TransactionDirection, TransactionInfo, WalletCoreBalance,
};
use crate::config::AppConfig;

type SpvClient = DashSpvClient<
    WalletManager<ManagedWalletInfo>,
    PeerNetworkManager,
    DiskStorageManager,
>;

pub(crate) struct NativeBackend {
    config: AppConfig,
    wallet: Arc<tokio::sync::RwLock<WalletManager<ManagedWalletInfo>>>,
    running: AtomicBool,
    event_tx: EventSender,
    progress: Arc<RwLock<SyncProgress>>,
    shutdown_token: tokio::sync::Mutex<Option<CancellationToken>>,
    client_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    bridge_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl NativeBackend {
    pub(crate) fn new(config: AppConfig) -> Self {
        let wallet_manager = WalletManager::<ManagedWalletInfo>::new(config.network);
        let wallet = Arc::new(tokio::sync::RwLock::new(wallet_manager));
        let (event_tx, _) = event_channel(256);

        Self {
            config,
            wallet,
            running: AtomicBool::new(false),
            event_tx,
            progress: Arc::new(RwLock::new(SyncProgress::default())),
            shutdown_token: tokio::sync::Mutex::new(None),
            client_task: tokio::sync::Mutex::new(None),
            bridge_task: tokio::sync::Mutex::new(None),
        }
    }
}

impl SpvBackend for NativeBackend {
    async fn start(&self) -> BackendResult<()> {
        if self.running.load(Ordering::Relaxed) {
            return Err(BackendError::AlreadyRunning);
        }

        let client_config = ClientConfig::new(self.config.network)
            .with_storage_path(self.config.data_dir.clone())
            .with_user_agent("dash-spv-ui");

        let network_manager = PeerNetworkManager::new(&client_config)
            .await
            .map_err(|e| BackendError::Sync(e.to_string()))?;

        let storage_manager = DiskStorageManager::new(&client_config)
            .await
            .map_err(|e| BackendError::Storage(e.to_string()))?;

        let client = SpvClient::new(
            client_config,
            network_manager,
            storage_manager,
            self.wallet.clone(),
        )
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))?;

        let progress_rx = client.subscribe_progress().await;
        let sync_rx = client.subscribe_sync_events().await;
        let network_rx = client.subscribe_network_events().await;
        let wallet_rx = self.wallet.read().await.subscribe_events();

        let token = CancellationToken::new();

        // Spawn event bridge task
        let bridge_token = token.clone();
        let event_tx = self.event_tx.clone();
        let cached_progress = self.progress.clone();
        let bridge_handle = tokio::spawn(
            event_bridge(bridge_token, progress_rx, sync_rx, network_rx, wallet_rx, event_tx, cached_progress),
        );

        // Spawn client run task
        let run_token = token.clone();
        let client_handle = tokio::spawn(async move {
            if let Err(e) = client.run(run_token).await {
                tracing::error!("SPV client error: {e}");
            }
        });

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

        self.running.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn pause(&self) -> BackendResult<()> {
        self.stop().await
    }

    async fn resume(&self) -> BackendResult<()> {
        self.start().await
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
        self.progress
            .read()
            .map(|p| p.clone())
            .unwrap_or_default()
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
                WalletAccountCreationOptions::default(),
            )
            .map_err(|e| match e {
                key_wallet::manager::WalletError::InvalidMnemonic(msg) => {
                    BackendError::InvalidMnemonic(msg)
                }
                key_wallet::manager::WalletError::WalletExists(_) => {
                    BackendError::WalletAlreadyExists
                }
                other => BackendError::Internal(other.to_string()),
            })?;
        Ok(())
    }

    async fn load_wallet(&self) -> BackendResult<bool> {
        Ok(false)
    }

    fn get_receive_address(&self) -> BackendResult<String> {
        let wallet = self.wallet.try_read().map_err(|_| {
            BackendError::Internal("wallet lock contention".to_string())
        })?;

        let wallet_ids: Vec<_> = wallet.list_wallets().into_iter().cloned().collect();
        let wallet_id = wallet_ids.first().ok_or(BackendError::NoWallet)?;
        drop(wallet);

        let mut wallet = self.wallet.try_write().map_err(|_| {
            BackendError::Internal("wallet lock contention".to_string())
        })?;

        let result = wallet
            .get_receive_address(
                wallet_id,
                0,
                AccountTypePreference::PreferBIP44,
                true,
            )
            .map_err(|e| BackendError::Internal(e.to_string()))?;

        result
            .address
            .map(|a| a.to_string())
            .ok_or_else(|| BackendError::Internal("no address generated".to_string()))
    }

    fn get_balance(&self) -> BackendResult<WalletCoreBalance> {
        let wallet = self.wallet.try_read().map_err(|_| {
            BackendError::Internal("wallet lock contention".to_string())
        })?;

        let wallet_ids: Vec<_> = wallet.list_wallets().into_iter().cloned().collect();
        let wallet_id = wallet_ids.first().ok_or(BackendError::NoWallet)?;

        wallet
            .get_wallet_balance(wallet_id)
            .map_err(|e| BackendError::Internal(e.to_string()))
    }

    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>> {
        let wallet = self.wallet.try_read().map_err(|_| {
            BackendError::Internal("wallet lock contention".to_string())
        })?;

        let wallet_ids: Vec<_> = wallet.list_wallets().into_iter().cloned().collect();
        let wallet_id = wallet_ids.first().ok_or(BackendError::NoWallet)?;

        let records = wallet
            .wallet_transaction_history(wallet_id)
            .map_err(|e| BackendError::Internal(e.to_string()))?;

        let transactions = records
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
                    is_instant_send: false,
                    is_chain_locked: false,
                }
            })
            .collect();

        Ok(transactions)
    }

    async fn send(&self, _address: &str, _amount: u64) -> BackendResult<[u8; 32]> {
        Err(BackendError::Internal("not implemented".into()))
    }

    fn subscribe_events(&self) -> EventReceiver {
        self.event_tx.subscribe()
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
        NetworkEvent::PeerConnected { address } => {
            SpvEvent::PeerConnected(address.to_string())
        }
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
            ..
        } => {
            use dashcore::hashes::Hash;
            SpvEvent::TransactionReceived {
                txid: txid.to_byte_array(),
                amount,
                addresses: addresses.iter().map(|a| a.to_string()).collect(),
            }
        }
        WalletEvent::TransactionStatusChanged { .. } => {
            // Status changes (confirmation, IS-lock) don't have a direct SpvEvent mapping yet.
            // Skip by returning a no-op error event.
            SpvEvent::Error("transaction status changed (unmapped)".to_string())
        }
        WalletEvent::BalanceUpdated {
            spendable,
            unconfirmed,
            immature,
            locked,
            ..
        } => SpvEvent::BalanceUpdated(WalletCoreBalance::new(
            spendable, unconfirmed, immature, locked,
        )),
    }
}
