use std::collections::BTreeSet;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use dash_spv::sync::{
    BlockHeadersProgress, BlocksProgress, ChainLockProgress, FilterHeadersProgress,
    FiltersProgress, InstantSendProgress, MasternodesProgress, MempoolProgress, SyncState,
};
use dash_spv_ffi::callbacks::{
    FFIClientErrorCallback, FFIEventCallbacks, FFINetworkEventCallbacks, FFIProgressCallback,
    FFISyncEventCallbacks, FFIWalletEventCallbacks,
};
use dash_spv_ffi::client::{
    FFIDashSpvClient, dash_spv_ffi_client_broadcast_transaction, dash_spv_ffi_client_destroy,
    dash_spv_ffi_client_get_wallet_manager, dash_spv_ffi_client_new, dash_spv_ffi_client_run,
    dash_spv_ffi_client_stop, dash_spv_ffi_wallet_manager_free,
};
use dash_spv_ffi::config::{
    dash_spv_ffi_config_add_peer, dash_spv_ffi_config_clear_peers, dash_spv_ffi_config_destroy,
    dash_spv_ffi_config_new, dash_spv_ffi_config_set_data_dir,
    dash_spv_ffi_config_set_masternode_sync_enabled,
    dash_spv_ffi_config_set_restrict_to_configured_peers, dash_spv_ffi_config_set_user_agent,
};
use dash_spv_ffi::error::dash_spv_ffi_get_last_error;
use dash_spv_ffi::types::{
    FFIBlockHeadersProgress, FFIBlocksProgress, FFIChainLockProgress, FFIFilterHeadersProgress,
    FFIFiltersProgress, FFIInstantSendProgress, FFIMasternodesProgress, FFIMempoolProgress,
    FFISyncProgress,
};
use dashcore::hashes::Hash;
use key_wallet::DerivationPathBuilder;
use key_wallet::managed_account::managed_account_type::ManagedAccountType;
use key_wallet::wallet::initialization::WalletAccountCreationOptions;
use key_wallet::wallet::managed_wallet_info::ManagedWalletInfo;
use key_wallet::wallet::managed_wallet_info::coin_selection::SelectionStrategy;
use key_wallet::wallet::managed_wallet_info::fee::FeeRate;
use key_wallet::wallet::managed_wallet_info::transaction_builder::TransactionBuilder;
use key_wallet::wallet::managed_wallet_info::wallet_info_interface::WalletInfoInterface;
use key_wallet_ffi::error::FFIError as WalletFFIError;
use key_wallet_ffi::managed_account::{
    FFITransactionRecord, managed_core_account_free, managed_core_account_free_transactions,
    managed_core_account_get_transactions, managed_wallet_get_account,
};
use key_wallet_ffi::managed_wallet::{
    managed_wallet_get_next_bip44_change_address, managed_wallet_info_free,
};
use key_wallet_ffi::mnemonic::{mnemonic_free, mnemonic_generate};
use key_wallet_ffi::types::{
    FFIAccountType, FFINetwork, FFIOutputRole, FFITransactionContextType, FFITransactionDirection,
    FFITransactionType,
};
use key_wallet_ffi::utxo::{managed_wallet_get_utxos, utxo_array_free};
use key_wallet_ffi::wallet::wallet_free_const;
use key_wallet_ffi::wallet_manager::{
    wallet_manager_add_wallet_from_mnemonic, wallet_manager_free_wallet_ids,
    wallet_manager_get_managed_wallet_info, wallet_manager_get_wallet,
    wallet_manager_get_wallet_balance, wallet_manager_get_wallet_ids,
};
use key_wallet_manager::WalletManager;

use super::error::{BackendError, BackendResult};
use super::events::{EventReceiver, EventSender, SpvEvent, event_channel};
use super::r#trait::SpvBackend;
use super::types::{
    InputInfo, Network, OutputInfo, OutputRole, SyncProgress, TransactionDirection,
    TransactionInfo, TransactionType, WalletCoreBalance,
};
use crate::config::AppConfig;

/// Context passed as `user_data` to all FFI callbacks.
struct CallbackContext {
    event_tx: EventSender,
    progress: std::sync::Arc<RwLock<SyncProgress>>,
    network: Network,
}

pub struct FfiBackend {
    config: AppConfig,
    client_ptr: Mutex<*mut FFIDashSpvClient>,
    running: AtomicBool,
    event_tx: EventSender,
    progress: std::sync::Arc<RwLock<SyncProgress>>,
    callback_ctx: Mutex<Option<*mut c_void>>,
}

// Safety: FFI pointers are only accessed behind Mutex, and the FFI client
// is thread-safe (it uses internal synchronization).
unsafe impl Send for FfiBackend {}
unsafe impl Sync for FfiBackend {}

impl FfiBackend {
    pub fn new(config: AppConfig) -> Self {
        let (event_tx, _) = event_channel(4096);

        Self {
            config,
            client_ptr: Mutex::new(std::ptr::null_mut()),
            running: AtomicBool::new(false),
            event_tx,
            progress: std::sync::Arc::new(RwLock::new(SyncProgress::default())),
            callback_ctx: Mutex::new(None),
        }
    }

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

impl SpvBackend for FfiBackend {
    async fn start(&self) -> BackendResult<()> {
        if self.running.load(Ordering::Relaxed) {
            return Err(BackendError::AlreadyRunning);
        }

        let app_network = self.config.network;
        let network = network_to_ffi(app_network);
        let data_dir = self.config.network_data_dir().display().to_string();
        let peers = self.config.peers().to_vec();
        let event_tx = self.event_tx.clone();
        let progress = self.progress.clone();

        // Run all FFI calls on a blocking thread to avoid Tokio runtime nesting.
        // Raw pointers are not Send, so we transmit them as usize.
        let (client_usize, user_data_usize) = tokio::task::spawn_blocking(move || {
            let config_ptr = dash_spv_ffi_config_new(network);
            if config_ptr.is_null() {
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            let c_data_dir =
                CString::new(data_dir).map_err(|e| BackendError::Internal(e.to_string()))?;
            // Safety: config_ptr is valid (just created above), c_data_dir is a valid C string.
            let result =
                unsafe { dash_spv_ffi_config_set_data_dir(config_ptr, c_data_dir.as_ptr()) };
            if result != 0 {
                // Safety: config_ptr is valid.
                unsafe { dash_spv_ffi_config_destroy(config_ptr) };
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            let c_user_agent = c"dash-spv-ui-ffi";
            // Safety: config_ptr is valid, c_user_agent is a static C string literal.
            let result =
                unsafe { dash_spv_ffi_config_set_user_agent(config_ptr, c_user_agent.as_ptr()) };
            if result != 0 {
                // Safety: config_ptr is valid.
                unsafe { dash_spv_ffi_config_destroy(config_ptr) };
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            if app_network == Network::Regtest {
                // Safety: config_ptr is valid.
                unsafe {
                    dash_spv_ffi_config_set_masternode_sync_enabled(config_ptr, false);
                };
            }

            if !peers.is_empty() {
                // Safety: config_ptr is valid.
                unsafe { dash_spv_ffi_config_clear_peers(config_ptr) };

                for peer in &peers {
                    let c_peer = CString::new(peer.as_str())
                        .map_err(|e| BackendError::Internal(e.to_string()))?;
                    // Safety: config_ptr is valid, c_peer is a valid C string.
                    let result =
                        unsafe { dash_spv_ffi_config_add_peer(config_ptr, c_peer.as_ptr()) };
                    if result != 0 {
                        unsafe { dash_spv_ffi_config_destroy(config_ptr) };
                        return Err(BackendError::Internal(get_last_ffi_error()));
                    }
                }

                // Safety: config_ptr is valid.
                unsafe {
                    dash_spv_ffi_config_set_restrict_to_configured_peers(config_ptr, true);
                };
            }

            let ctx = Box::new(CallbackContext {
                event_tx,
                progress,
                network: app_network,
            });
            let user_data = Box::into_raw(ctx) as *mut c_void;

            let callbacks = FFIEventCallbacks {
                sync: build_sync_callbacks(user_data),
                network: build_network_callbacks(user_data),
                progress: build_progress_callback(user_data),
                wallet: build_wallet_callbacks(user_data),
                error: build_error_callback(user_data),
            };

            // Safety: config_ptr is a valid FFIClientConfig pointer, callbacks
            // contain valid function pointers with user_data pointing to a
            // valid CallbackContext.
            let client_ptr = unsafe { dash_spv_ffi_client_new(config_ptr, callbacks) };

            // Config is consumed by client_new; destroy it regardless.
            // Safety: config_ptr is valid.
            unsafe { dash_spv_ffi_config_destroy(config_ptr) };

            if client_ptr.is_null() {
                // Safety: user_data was created by Box::into_raw above.
                let _ = unsafe { Box::from_raw(user_data as *mut CallbackContext) };
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            // Safety: client_ptr is valid, callbacks are set.
            let result = unsafe { dash_spv_ffi_client_run(client_ptr) };
            if result != 0 {
                // Safety: user_data was created by Box::into_raw above.
                let _ = unsafe { Box::from_raw(user_data as *mut CallbackContext) };
                // Safety: client_ptr is valid.
                unsafe { dash_spv_ffi_client_destroy(client_ptr) };
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            Ok((client_ptr as usize, user_data as usize))
        })
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))??;

        // Safety: usize values were cast from valid pointers inside spawn_blocking.
        let client_ptr = client_usize as *mut FFIDashSpvClient;
        let user_data = user_data_usize as *mut c_void;

        *self.client_ptr.lock().unwrap() = client_ptr;
        *self.callback_ctx.lock().unwrap() = Some(user_data);
        self.running.store(true, Ordering::Relaxed);

        // Load any previously saved wallet into the running client.
        let _ = self.load_wallet().await;

        Ok(())
    }

    async fn stop(&self) -> BackendResult<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(BackendError::NotRunning);
        }

        let client_usize = {
            let mut guard = self.client_ptr.lock().unwrap();
            let ptr = *guard;
            *guard = std::ptr::null_mut();
            ptr as usize
        };

        let user_data_usize = self.callback_ctx.lock().unwrap().take().map(|p| p as usize);

        // Run FFI teardown on a blocking thread to avoid Tokio runtime nesting.
        tokio::task::spawn_blocking(move || {
            if client_usize != 0 {
                // Safety: client_usize was cast from a valid FFIDashSpvClient pointer.
                let client_ptr = client_usize as *mut FFIDashSpvClient;
                unsafe {
                    dash_spv_ffi_client_stop(client_ptr);
                    dash_spv_ffi_client_destroy(client_ptr);
                }
            }

            // Reclaim callback context after the client is fully stopped
            if let Some(ud) = user_data_usize {
                // Safety: ud was cast from a pointer created by Box::into_raw in start().
                let _ = unsafe { Box::from_raw(ud as *mut CallbackContext) };
            }
        })
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))?;

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
        let mut error = WalletFFIError::success();
        let ptr = mnemonic_generate(12, &mut error);
        if ptr.is_null() {
            return Err(BackendError::Internal(read_wallet_ffi_error(&error)));
        }
        // Safety: ptr was returned by mnemonic_generate and is a valid C string.
        let phrase = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        // Safety: ptr was allocated by mnemonic_generate (CString::into_raw).
        unsafe { mnemonic_free(ptr) };
        Ok(phrase)
    }

    async fn create_wallet(&self, mnemonic: &str) -> BackendResult<()> {
        let client_usize = *self.client_ptr.lock().unwrap() as usize;
        if client_usize == 0 {
            // If the client isn't running yet, just save the mnemonic for later.
            self.save_mnemonic(mnemonic)?;
            return Ok(());
        }

        let c_mnemonic =
            CString::new(mnemonic).map_err(|e| BackendError::Internal(e.to_string()))?;

        // Run FFI calls on a blocking thread to avoid Tokio runtime nesting.
        tokio::task::spawn_blocking(move || {
            // Safety: client_usize was cast from a valid FFIDashSpvClient pointer.
            let client_ptr = client_usize as *mut FFIDashSpvClient;
            let wm = unsafe { dash_spv_ffi_client_get_wallet_manager(client_ptr) };
            if wm.is_null() {
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            let mut error = WalletFFIError::success();

            // Safety: wm is valid (just obtained), c_mnemonic is a valid C string,
            // passphrase is null (empty passphrase), error is a valid stack variable.
            let ok = unsafe {
                wallet_manager_add_wallet_from_mnemonic(
                    wm as *mut key_wallet_ffi::FFIWalletManager,
                    c_mnemonic.as_ptr(),
                    std::ptr::null(),
                    &mut error,
                )
            };

            // Safety: wm was obtained from dash_spv_ffi_client_get_wallet_manager.
            unsafe { dash_spv_ffi_wallet_manager_free(wm) };

            if !ok {
                return Err(BackendError::Internal(read_wallet_ffi_error(&error)));
            }

            Ok(())
        })
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))??;

        self.save_mnemonic(mnemonic)?;
        Ok(())
    }

    async fn load_wallet(&self) -> BackendResult<bool> {
        let mnemonic = match self.read_mnemonic()? {
            Some(m) => m,
            None => return Ok(false),
        };

        let client_usize = *self.client_ptr.lock().unwrap() as usize;
        if client_usize == 0 {
            // Client not running yet, mnemonic exists so we signal wallet is loadable
            return Ok(true);
        }

        let c_mnemonic =
            CString::new(mnemonic.as_str()).map_err(|e| BackendError::Internal(e.to_string()))?;

        // Run FFI calls on a blocking thread to avoid Tokio runtime nesting.
        tokio::task::spawn_blocking(move || {
            // Safety: client_usize was cast from a valid FFIDashSpvClient pointer.
            let client_ptr = client_usize as *mut FFIDashSpvClient;
            let wm = unsafe { dash_spv_ffi_client_get_wallet_manager(client_ptr) };
            if wm.is_null() {
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            let mut error = WalletFFIError::success();

            // Safety: wm is valid, c_mnemonic is a valid C string.
            let ok = unsafe {
                wallet_manager_add_wallet_from_mnemonic(
                    wm as *mut key_wallet_ffi::FFIWalletManager,
                    c_mnemonic.as_ptr(),
                    std::ptr::null(),
                    &mut error,
                )
            };

            // Safety: wm was obtained from dash_spv_ffi_client_get_wallet_manager.
            unsafe { dash_spv_ffi_wallet_manager_free(wm) };

            if !ok {
                let msg = read_wallet_ffi_error(&error);
                if msg.contains("already exists") {
                    return Ok(true);
                }
                return Err(BackendError::Internal(msg));
            }

            Ok(true)
        })
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))?
    }

    fn get_receive_address(&self) -> BackendResult<String> {
        Err(BackendError::Internal(
            "receive address not yet implemented via FFI".to_string(),
        ))
    }

    fn get_balance(&self) -> BackendResult<WalletCoreBalance> {
        let client_usize = *self.client_ptr.lock().unwrap() as usize;
        if client_usize == 0 {
            return Err(BackendError::NotRunning);
        }

        // Run all FFI calls on a separate OS thread to avoid Tokio runtime
        // nesting — wallet manager functions internally call `block_on`.
        std::thread::spawn(move || {
            let client_ptr = client_usize as *mut FFIDashSpvClient;

            // First, get the wallet ID
            let wm = unsafe { dash_spv_ffi_client_get_wallet_manager(client_ptr) };
            if wm.is_null() {
                return Err(BackendError::Internal(get_last_ffi_error()));
            }

            let mut wallet_ids_ptr: *mut u8 = std::ptr::null_mut();
            let mut count: usize = 0;
            let mut error = WalletFFIError::success();

            let ok = unsafe {
                wallet_manager_get_wallet_ids(
                    wm as *const key_wallet_ffi::FFIWalletManager,
                    &mut wallet_ids_ptr,
                    &mut count,
                    &mut error,
                )
            };

            if !ok || count == 0 {
                unsafe { dash_spv_ffi_wallet_manager_free(wm) };
                return Err(BackendError::NoWallet);
            }

            let mut wallet_id = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(wallet_ids_ptr, wallet_id.as_mut_ptr(), 32);
                wallet_manager_free_wallet_ids(wallet_ids_ptr, count);
            }

            // Now get the balance
            let mut confirmed: u64 = 0;
            let mut unconfirmed: u64 = 0;
            error = WalletFFIError::success();

            let ok = unsafe {
                wallet_manager_get_wallet_balance(
                    wm as *const key_wallet_ffi::FFIWalletManager,
                    wallet_id.as_ptr(),
                    &mut confirmed,
                    &mut unconfirmed,
                    &mut error,
                )
            };

            unsafe { dash_spv_ffi_wallet_manager_free(wm) };

            if !ok {
                return Err(BackendError::Internal(read_wallet_ffi_error(&error)));
            }

            Ok(WalletCoreBalance::new(confirmed, unconfirmed, 0, 0))
        })
        .join()
        .map_err(|_| BackendError::Internal("FFI thread panicked".into()))?
    }

    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>> {
        let client_usize = *self.client_ptr.lock().unwrap() as usize;
        if client_usize == 0 {
            return Err(BackendError::NotRunning);
        }

        let network = self.config.network;
        std::thread::spawn(move || {
            let client_ptr = client_usize as *mut FFIDashSpvClient;

            // Safety: client_ptr was cast from a valid pointer.
            let wm = unsafe { dash_spv_ffi_client_get_wallet_manager(client_ptr) };
            if wm.is_null() {
                return Err(BackendError::Internal(get_last_ffi_error()));
            }
            let wm_typed = wm as *const key_wallet_ffi::FFIWalletManager;

            let mut wallet_ids_ptr: *mut u8 = std::ptr::null_mut();
            let mut count: usize = 0;
            let mut error = WalletFFIError::success();

            let ok = unsafe {
                wallet_manager_get_wallet_ids(wm_typed, &mut wallet_ids_ptr, &mut count, &mut error)
            };

            if !ok || count == 0 {
                unsafe { dash_spv_ffi_wallet_manager_free(wm) };
                return Err(BackendError::NoWallet);
            }

            let mut wallet_id = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(wallet_ids_ptr, wallet_id.as_mut_ptr(), 32);
                wallet_manager_free_wallet_ids(wallet_ids_ptr, count);
            }

            // Get BIP44 account (index 0) to access transaction records
            let account_result = unsafe {
                managed_wallet_get_account(
                    wm_typed,
                    wallet_id.as_ptr(),
                    0,
                    FFIAccountType::StandardBIP44,
                )
            };

            if account_result.account.is_null() {
                unsafe { dash_spv_ffi_wallet_manager_free(wm) };
                return Err(BackendError::Internal(
                    "failed to get standard account".to_string(),
                ));
            }

            let account_ptr = account_result.account;

            let mut txs_ptr: *mut FFITransactionRecord = std::ptr::null_mut();
            let mut tx_count: usize = 0;

            // Safety: account_ptr is valid (returned by managed_wallet_get_account).
            let ok = unsafe {
                managed_core_account_get_transactions(account_ptr, &mut txs_ptr, &mut tx_count)
            };

            if !ok {
                unsafe {
                    managed_core_account_free(account_ptr);
                    dash_spv_ffi_wallet_manager_free(wm);
                }
                return Err(BackendError::Internal(
                    "failed to get transactions".to_string(),
                ));
            }

            let mut transactions = Vec::with_capacity(tx_count);

            if !txs_ptr.is_null() && tx_count > 0 {
                // Safety: txs_ptr and tx_count were returned by
                // managed_core_account_get_transactions.
                let records = unsafe { std::slice::from_raw_parts(txs_ptr, tx_count) };

                for record in records {
                    let txid = dashcore::Txid::from_byte_array(record.txid);
                    let fee = if record.fee > 0 {
                        Some(record.fee)
                    } else {
                        None
                    };

                    let block_info = &record.context.block_info;
                    let has_block = block_info.block_hash != [0u8; 32] && block_info.timestamp != 0;
                    let is_instant_send = matches!(
                        record.context.context_type,
                        FFITransactionContextType::InstantSend
                    );
                    let is_chain_locked = matches!(
                        record.context.context_type,
                        FFITransactionContextType::InChainLockedBlock
                    );

                    // Safety: label is a valid C string for the lifetime of the record.
                    let label = unsafe { extract_ffi_label(record.label) };

                    let inputs = extract_ffi_inputs(record);
                    let outputs = extract_ffi_outputs(record, network);

                    transactions.push(TransactionInfo {
                        txid,
                        amount: record.net_amount,
                        direction: ffi_direction_to_direction(record.direction),
                        transaction_type: ffi_type_to_type(record.transaction_type),
                        timestamp: if has_block {
                            block_info.timestamp as u64
                        } else {
                            0
                        },
                        height: if has_block {
                            Some(block_info.height)
                        } else {
                            None
                        },
                        fee,
                        addresses: Vec::new(),
                        block_hash: if has_block {
                            Some(dashcore::BlockHash::from_byte_array(block_info.block_hash))
                        } else {
                            None
                        },
                        is_instant_send,
                        is_chain_locked,
                        label,
                        inputs,
                        outputs,
                    });
                }

                // Safety: txs_ptr and tx_count were returned by
                // managed_core_account_get_transactions.
                unsafe { managed_core_account_free_transactions(txs_ptr, tx_count) };
            }

            unsafe {
                managed_core_account_free(account_ptr);
                dash_spv_ffi_wallet_manager_free(wm);
            }

            // Sort: unconfirmed first, then by timestamp descending
            transactions.sort_by(|a, b| match (a.height.is_some(), b.height.is_some()) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => b.timestamp.cmp(&a.timestamp),
            });

            Ok(transactions)
        })
        .join()
        .map_err(|_| BackendError::Internal("FFI thread panicked".into()))?
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

        // Load mnemonic and create a local wallet for key derivation
        let mnemonic_str = self.read_mnemonic()?.ok_or(BackendError::NoWallet)?;

        let network = self.config.network;
        let mut local_wm = WalletManager::<ManagedWalletInfo>::new(network);
        let wallet_id = local_wm
            .create_wallet_from_mnemonic(
                &mnemonic_str,
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

        let client_usize = *self.client_ptr.lock().unwrap() as usize;
        if client_usize == 0 {
            return Err(BackendError::NotRunning);
        }

        // Get UTXOs and change address from the FFI wallet manager.
        // These FFI calls internally use `block_on`, so run on a separate OS thread.
        let (utxos, change_address_str) = std::thread::spawn(move || {
            let client_ptr = client_usize as *mut FFIDashSpvClient;

            // Safety: client_ptr was cast from a valid pointer.
            let wm = unsafe { dash_spv_ffi_client_get_wallet_manager(client_ptr) };
            if wm.is_null() {
                return Err(BackendError::Internal(get_last_ffi_error()));
            }
            let wm_typed = wm as *const key_wallet_ffi::FFIWalletManager;

            // Get wallet ID from FFI
            let mut wallet_ids_ptr: *mut u8 = std::ptr::null_mut();
            let mut count: usize = 0;
            let mut error = WalletFFIError::success();

            let ok = unsafe {
                wallet_manager_get_wallet_ids(wm_typed, &mut wallet_ids_ptr, &mut count, &mut error)
            };

            if !ok || count == 0 {
                unsafe { dash_spv_ffi_wallet_manager_free(wm) };
                return Err(BackendError::NoWallet);
            }

            let mut ffi_wallet_id = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(wallet_ids_ptr, ffi_wallet_id.as_mut_ptr(), 32);
                wallet_manager_free_wallet_ids(wallet_ids_ptr, count);
            }

            // Get managed wallet info for UTXOs
            error = WalletFFIError::success();
            let managed_info = unsafe {
                wallet_manager_get_managed_wallet_info(wm_typed, ffi_wallet_id.as_ptr(), &mut error)
            };

            if managed_info.is_null() {
                unsafe { dash_spv_ffi_wallet_manager_free(wm) };
                return Err(BackendError::Internal(read_wallet_ffi_error(&error)));
            }

            // Get UTXOs
            let mut utxos_ptr: *mut key_wallet_ffi::utxo::FFIUTXO = std::ptr::null_mut();
            let mut utxo_count: usize = 0;
            error = WalletFFIError::success();

            let ok = unsafe {
                managed_wallet_get_utxos(managed_info, &mut utxos_ptr, &mut utxo_count, &mut error)
            };

            if !ok {
                unsafe {
                    managed_wallet_info_free(managed_info);
                    dash_spv_ffi_wallet_manager_free(wm);
                }
                return Err(BackendError::Internal(read_wallet_ffi_error(&error)));
            }

            // Convert FFI UTXOs to Rust Utxo
            let mut utxos = Vec::with_capacity(utxo_count);
            if !utxos_ptr.is_null() && utxo_count > 0 {
                let ffi_utxos = unsafe { std::slice::from_raw_parts(utxos_ptr, utxo_count) };
                for ffi_utxo in ffi_utxos {
                    let txid = dashcore::Txid::from_byte_array(ffi_utxo.txid);
                    let outpoint = dashcore::OutPoint::new(txid, ffi_utxo.vout);

                    let script_bytes = if !ffi_utxo.script_pubkey.is_null()
                        && ffi_utxo.script_len > 0
                    {
                        unsafe {
                            std::slice::from_raw_parts(ffi_utxo.script_pubkey, ffi_utxo.script_len)
                        }
                        .to_vec()
                    } else {
                        Vec::new()
                    };
                    let script_pubkey = dashcore::ScriptBuf::from(script_bytes);

                    let address_str = if !ffi_utxo.address.is_null() {
                        unsafe { CStr::from_ptr(ffi_utxo.address) }
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        String::new()
                    };

                    let address = match dashcore::Address::from_str(&address_str) {
                        Ok(a) => a.assume_checked(),
                        Err(_) => continue,
                    };

                    let txout = dashcore::TxOut {
                        value: ffi_utxo.amount,
                        script_pubkey,
                    };

                    utxos.push(key_wallet::Utxo {
                        outpoint,
                        txout,
                        address,
                        height: ffi_utxo.height,
                        is_coinbase: false,
                        is_confirmed: ffi_utxo.confirmations > 0,
                        is_instantlocked: false,
                        is_locked: false,
                    });
                }

                unsafe { utxo_array_free(utxos_ptr, utxo_count) };
            }

            // Get wallet for change address generation
            error = WalletFFIError::success();
            let ffi_wallet =
                unsafe { wallet_manager_get_wallet(wm_typed, ffi_wallet_id.as_ptr(), &mut error) };

            if ffi_wallet.is_null() {
                unsafe {
                    managed_wallet_info_free(managed_info);
                    dash_spv_ffi_wallet_manager_free(wm);
                }
                return Err(BackendError::Internal(read_wallet_ffi_error(&error)));
            }

            // Get change address
            error = WalletFFIError::success();
            let change_addr_ptr = unsafe {
                managed_wallet_get_next_bip44_change_address(
                    managed_info,
                    ffi_wallet,
                    0,
                    &mut error,
                )
            };

            let change_address = if change_addr_ptr.is_null() {
                unsafe {
                    wallet_free_const(ffi_wallet);
                    managed_wallet_info_free(managed_info);
                    dash_spv_ffi_wallet_manager_free(wm);
                }
                return Err(BackendError::Internal(read_wallet_ffi_error(&error)));
            } else {
                let addr = unsafe { CStr::from_ptr(change_addr_ptr) }
                    .to_string_lossy()
                    .into_owned();
                unsafe {
                    key_wallet_ffi::wallet_manager::wallet_manager_free_string(change_addr_ptr);
                }
                addr
            };

            unsafe {
                wallet_free_const(ffi_wallet);
                managed_wallet_info_free(managed_info);
                dash_spv_ffi_wallet_manager_free(wm);
            }

            Ok((utxos, change_address))
        })
        .join()
        .map_err(|_| BackendError::Internal("FFI thread panicked".into()))??;

        if utxos.is_empty() {
            return Err(BackendError::InsufficientFunds {
                available: 0,
                required: amount,
            });
        }

        // Parse change address
        let change_address = dashcore::Address::from_str(&change_address_str)
            .map_err(|e| BackendError::Internal(format!("invalid change address: {e}")))?
            .assume_checked();

        // Get wallet and account info for key derivation from the local wallet manager
        let (wallet, info) = local_wm
            .get_wallet_and_info(&wallet_id)
            .ok_or(BackendError::NoWallet)?;

        let tip_height = self.tip_height().unwrap_or(0);
        let accounts = info.accounts();
        let coin_type = coin_type_for_network(network);

        // Build the key provider closure that derives private keys for UTXOs
        let key_provider = |utxo: &key_wallet::Utxo| -> Option<dashcore::secp256k1::SecretKey> {
            for (account_index, account) in &accounts.standard_bip44_accounts {
                if let ManagedAccountType::Standard {
                    external_addresses,
                    internal_addresses,
                    ..
                } = &account.account_type
                {
                    if let Some(addr_idx) = external_addresses.address_index(&utxo.address) {
                        let path = DerivationPathBuilder::new()
                            .coin_type(coin_type)
                            .account(*account_index)
                            .change(0)
                            .address_index(addr_idx)
                            .bip44()
                            .ok()?;
                        return wallet.derive_private_key(&path).ok();
                    }
                    if let Some(addr_idx) = internal_addresses.address_index(&utxo.address) {
                        let path = DerivationPathBuilder::new()
                            .coin_type(coin_type)
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
            .map_err(builder_error_to_backend)?;

        let tx = builder.build().map_err(builder_error_to_backend)?;

        let txid = tx.txid();

        // Serialize and broadcast via FFI
        let tx_bytes = dashcore::consensus::serialize(&tx);
        let client_usize = *self.client_ptr.lock().unwrap() as usize;
        if client_usize == 0 {
            return Err(BackendError::NotRunning);
        }

        tokio::task::spawn_blocking(move || {
            let client_ptr = client_usize as *mut FFIDashSpvClient;
            // Safety: client_ptr is valid, tx_bytes is a valid slice.
            let result = unsafe {
                dash_spv_ffi_client_broadcast_transaction(
                    client_ptr,
                    tx_bytes.as_ptr(),
                    tx_bytes.len(),
                )
            };
            if result != 0 {
                return Err(BackendError::Sync(get_last_ffi_error()));
            }
            Ok(())
        })
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))??;

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

/// `stop()` should be called before the backend is dropped. The FFI client's
/// `destroy` function internally calls `block_on()`, which panics if called
/// from within a tokio runtime (e.g., in tests or Dioxus). By requiring an
/// explicit `stop()` — which runs the teardown on `spawn_blocking` — we
/// avoid this runtime nesting issue entirely.
///
/// If `stop()` was not called (e.g., due to a panic), Drop leaks both the
/// FFI client and the callback context to avoid use-after-free — the leaked
/// client's background tasks may still reference the context via user_data.
impl Drop for FfiBackend {
    fn drop(&mut self) {
        let client_ptr = *self.client_ptr.lock().unwrap();
        if !client_ptr.is_null() {
            tracing::error!(
                "FfiBackend dropped without calling stop() — FFI client resources leaked. \
                 Always call stop() before dropping."
            );
            // Do NOT free callback_context here — the leaked client's background
            // tasks may still reference it via user_data pointers.
            return;
        }

        // Client was properly stopped (ptr is null) — safe to free callback context.
        if let Some(ud) = self.callback_ctx.lock().unwrap().take() {
            // Safety: ud was created by Box::into_raw(Box::new(CallbackContext {...})).
            let _ = unsafe { Box::from_raw(ud as *mut CallbackContext) };
        }
    }
}

// ============================================================================
// FFI helpers
// ============================================================================

fn get_last_ffi_error() -> String {
    let ptr = dash_spv_ffi_get_last_error();
    if ptr.is_null() {
        "unknown FFI error".to_string()
    } else {
        // Safety: ptr is returned by dash_spv_ffi_get_last_error and points to
        // a static mutex-guarded CString that remains valid for the duration of
        // this read.
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

fn read_wallet_ffi_error(error: &WalletFFIError) -> String {
    if error.message.is_null() {
        format!("wallet FFI error code {:?}", error.code)
    } else {
        // Safety: error.message was set by key-wallet-ffi and is a valid C string.
        unsafe { CStr::from_ptr(error.message) }
            .to_string_lossy()
            .into_owned()
    }
}

fn network_to_ffi(network: Network) -> FFINetwork {
    FFINetwork::from(network)
}

/// Return the BIP44 coin type for the given network.
fn coin_type_for_network(network: Network) -> u32 {
    match network {
        Network::Mainnet => 5,
        _ => 1,
    }
}

fn builder_error_to_backend(
    e: key_wallet::wallet::managed_wallet_info::transaction_builder::BuilderError,
) -> BackendError {
    use key_wallet::wallet::managed_wallet_info::coin_selection::SelectionError;
    use key_wallet::wallet::managed_wallet_info::transaction_builder::BuilderError;

    match e {
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
    }
}

fn ffi_sync_state_to_rust(state: dash_spv_ffi::types::FFISyncState) -> SyncState {
    match state {
        dash_spv_ffi::types::FFISyncState::WaitForEvents => SyncState::WaitForEvents,
        dash_spv_ffi::types::FFISyncState::WaitingForConnections => {
            SyncState::WaitingForConnections
        }
        dash_spv_ffi::types::FFISyncState::Syncing => SyncState::Syncing,
        dash_spv_ffi::types::FFISyncState::Synced => SyncState::Synced,
        dash_spv_ffi::types::FFISyncState::Error => SyncState::Error,
    }
}

/// Convert an `FFISyncProgress` pointer into a Rust `SyncProgress`.
///
/// Reads each non-null sub-progress pointer and constructs the corresponding
/// Rust progress type using its public setter methods.
///
/// # Safety
/// `ffi` must point to a valid `FFISyncProgress` struct. All non-null sub-progress
/// pointers within must also be valid.
unsafe fn ffi_progress_to_rust(ffi: *const FFISyncProgress) -> SyncProgress {
    // Safety: all dereferences below are valid because ffi and its non-null
    // sub-progress pointers were allocated by the FFI layer and remain valid
    // for the duration of this callback.
    unsafe {
        let ffi = &*ffi;
        let mut progress = SyncProgress::default();

        if !ffi.headers.is_null() {
            progress.update_headers(convert_headers_progress(&*ffi.headers));
        }
        if !ffi.filter_headers.is_null() {
            progress.update_filter_headers(convert_filter_headers_progress(&*ffi.filter_headers));
        }
        if !ffi.filters.is_null() {
            progress.update_filters(convert_filters_progress(&*ffi.filters));
        }
        if !ffi.blocks.is_null() {
            progress.update_blocks(convert_blocks_progress(&*ffi.blocks));
        }
        if !ffi.masternodes.is_null() {
            progress.update_masternodes(convert_masternodes_progress(&*ffi.masternodes));
        }
        if !ffi.chainlocks.is_null() {
            progress.update_chainlocks(convert_chainlocks_progress(&*ffi.chainlocks));
        }
        if !ffi.instantsend.is_null() {
            progress.update_instantsend(convert_instantsend_progress(&*ffi.instantsend));
        }
        if !ffi.mempool.is_null() {
            progress.update_mempool(convert_mempool_progress(&*ffi.mempool));
        }

        progress
    }
}

fn convert_headers_progress(ffi: &FFIBlockHeadersProgress) -> BlockHeadersProgress {
    let mut p = BlockHeadersProgress::default();
    p.set_state(ffi_sync_state_to_rust(ffi.state));
    p.update_tip_height(ffi.tip_height);
    p.update_target_height(ffi.target_height);
    p.update_buffered(ffi.buffered);
    p
}

fn convert_filter_headers_progress(ffi: &FFIFilterHeadersProgress) -> FilterHeadersProgress {
    let mut p = FilterHeadersProgress::default();
    p.set_state(ffi_sync_state_to_rust(ffi.state));
    p.update_current_height(ffi.current_height);
    p.update_target_height(ffi.target_height);
    p.update_block_header_tip_height(ffi.block_header_tip_height);
    p
}

fn convert_filters_progress(ffi: &FFIFiltersProgress) -> FiltersProgress {
    let mut p = FiltersProgress::default();
    p.set_state(ffi_sync_state_to_rust(ffi.state));
    p.update_committed_height(ffi.committed_height);
    p.update_stored_height(ffi.stored_height);
    p.update_target_height(ffi.target_height);
    p.update_filter_header_tip_height(ffi.filter_header_tip_height);
    p
}

fn convert_blocks_progress(ffi: &FFIBlocksProgress) -> BlocksProgress {
    let mut p = BlocksProgress::default();
    p.set_state(ffi_sync_state_to_rust(ffi.state));
    p
}

fn convert_masternodes_progress(ffi: &FFIMasternodesProgress) -> MasternodesProgress {
    let mut p = MasternodesProgress::default();
    p.set_state(ffi_sync_state_to_rust(ffi.state));
    p
}

fn convert_chainlocks_progress(ffi: &FFIChainLockProgress) -> ChainLockProgress {
    let mut p = ChainLockProgress::default();
    p.set_state(ffi_sync_state_to_rust(ffi.state));
    p
}

fn convert_instantsend_progress(ffi: &FFIInstantSendProgress) -> InstantSendProgress {
    let mut p = InstantSendProgress::default();
    p.set_state(ffi_sync_state_to_rust(ffi.state));
    p
}

fn convert_mempool_progress(_ffi: &FFIMempoolProgress) -> MempoolProgress {
    // MempoolProgress::set_state is pub(super), so we can only return a default.
    MempoolProgress::default()
}

// ============================================================================
// Callback builders
// ============================================================================

fn build_sync_callbacks(user_data: *mut c_void) -> FFISyncEventCallbacks {
    FFISyncEventCallbacks {
        on_sync_start: Some(on_sync_start),
        on_block_headers_stored: Some(on_block_headers_stored),
        on_block_header_sync_complete: Some(on_block_header_sync_complete),
        on_filter_headers_stored: None,
        on_filter_headers_sync_complete: None,
        on_filters_stored: None,
        on_filters_sync_complete: Some(on_filters_sync_complete),
        on_blocks_needed: None,
        on_block_processed: Some(on_block_processed),
        on_masternode_state_updated: None,
        on_chainlock_received: Some(on_chainlock_received),
        on_instantlock_received: Some(on_instantlock_received),
        on_manager_error: Some(on_manager_error),
        on_sync_complete: Some(on_sync_complete),
        user_data,
    }
}

fn build_network_callbacks(user_data: *mut c_void) -> FFINetworkEventCallbacks {
    FFINetworkEventCallbacks {
        on_peer_connected: Some(on_peer_connected),
        on_peer_disconnected: Some(on_peer_disconnected),
        on_peers_updated: Some(on_peers_updated),
        user_data,
    }
}

fn build_wallet_callbacks(user_data: *mut c_void) -> FFIWalletEventCallbacks {
    FFIWalletEventCallbacks {
        on_transaction_received: Some(on_transaction_received),
        on_transaction_status_changed: None,
        on_balance_updated: Some(on_balance_updated),
        user_data,
    }
}

fn build_progress_callback(user_data: *mut c_void) -> FFIProgressCallback {
    FFIProgressCallback {
        on_progress: Some(on_progress_update),
        user_data,
    }
}

fn build_error_callback(user_data: *mut c_void) -> FFIClientErrorCallback {
    FFIClientErrorCallback {
        on_error: Some(on_client_error),
        user_data,
    }
}

// ============================================================================
// Extern "C" callback implementations
// ============================================================================

extern "C" fn on_sync_start(
    manager_id: dash_spv_ffi::callbacks::FFIManagerId,
    user_data: *mut c_void,
) {
    // Safety: user_data is a valid CallbackContext pointer created by Box::into_raw
    // in start(). We only borrow it (no ownership transfer).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let manager = match manager_id {
        dash_spv_ffi::callbacks::FFIManagerId::Headers => {
            dash_spv::sync::ManagerIdentifier::BlockHeader
        }
        dash_spv_ffi::callbacks::FFIManagerId::FilterHeaders => {
            dash_spv::sync::ManagerIdentifier::FilterHeader
        }
        dash_spv_ffi::callbacks::FFIManagerId::Filters => dash_spv::sync::ManagerIdentifier::Filter,
        dash_spv_ffi::callbacks::FFIManagerId::Blocks => dash_spv::sync::ManagerIdentifier::Block,
        dash_spv_ffi::callbacks::FFIManagerId::Masternodes => {
            dash_spv::sync::ManagerIdentifier::Masternode
        }
        dash_spv_ffi::callbacks::FFIManagerId::ChainLocks => {
            dash_spv::sync::ManagerIdentifier::ChainLock
        }
        dash_spv_ffi::callbacks::FFIManagerId::InstantSend => {
            dash_spv::sync::ManagerIdentifier::InstantSend
        }
        dash_spv_ffi::callbacks::FFIManagerId::Mempool => {
            dash_spv::sync::ManagerIdentifier::Mempool
        }
    };
    let _ = ctx.event_tx.send(SpvEvent::SyncStarted { manager });
}

extern "C" fn on_block_headers_stored(tip_height: u32, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx.event_tx.send(SpvEvent::HeadersSynced { tip_height });
}

extern "C" fn on_block_header_sync_complete(tip_height: u32, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx.event_tx.send(SpvEvent::HeadersSynced { tip_height });
}

extern "C" fn on_filters_sync_complete(tip_height: u32, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx.event_tx.send(SpvEvent::FiltersSynced { tip_height });
}

extern "C" fn on_block_processed(
    height: u32,
    _hash: *const [u8; 32],
    new_address_count: u32,
    _confirmed_txids: *const [u8; 32],
    _confirmed_txid_count: u32,
    user_data: *mut c_void,
) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx.event_tx.send(SpvEvent::BlockProcessed {
        height,
        new_addresses: new_address_count,
    });
}

extern "C" fn on_chainlock_received(
    height: u32,
    _hash: *const [u8; 32],
    _signature: *const [u8; 96],
    validated: bool,
    user_data: *mut c_void,
) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx
        .event_tx
        .send(SpvEvent::ChainLockReceived { height, validated });
}

extern "C" fn on_instantlock_received(
    txid: *const [u8; 32],
    _instantlock_data: *const u8,
    _instantlock_len: usize,
    validated: bool,
    user_data: *mut c_void,
) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    // txid is a borrowed pointer valid for the duration of the callback.
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let txid_bytes = if txid.is_null() {
        [0u8; 32]
    } else {
        // Safety: txid is a valid pointer to a 32-byte array (callback contract).
        unsafe { *txid }
    };
    let _ = ctx.event_tx.send(SpvEvent::InstantLockReceived {
        txid: txid_bytes,
        validated,
    });
}

extern "C" fn on_manager_error(
    _manager_id: dash_spv_ffi::callbacks::FFIManagerId,
    error: *const c_char,
    user_data: *mut c_void,
) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let msg = if error.is_null() {
        "unknown manager error".to_string()
    } else {
        // Safety: error is a borrowed C string valid for the duration of the callback.
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    };
    let _ = ctx.event_tx.send(SpvEvent::Error(msg));
}

extern "C" fn on_sync_complete(header_tip: u32, cycle: u32, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx.event_tx.send(SpvEvent::SyncComplete {
        tip_height: header_tip,
        cycle,
    });
}

extern "C" fn on_peer_connected(address: *const c_char, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let addr = if address.is_null() {
        "unknown".to_string()
    } else {
        // Safety: address is a borrowed C string valid for the duration of the callback.
        unsafe { CStr::from_ptr(address) }
            .to_string_lossy()
            .into_owned()
    };
    let _ = ctx.event_tx.send(SpvEvent::PeerConnected(addr));
}

extern "C" fn on_peer_disconnected(address: *const c_char, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let addr = if address.is_null() {
        "unknown".to_string()
    } else {
        // Safety: address is a borrowed C string valid for the duration of the callback.
        unsafe { CStr::from_ptr(address) }
            .to_string_lossy()
            .into_owned()
    };
    let _ = ctx.event_tx.send(SpvEvent::PeerDisconnected(addr));
}

extern "C" fn on_peers_updated(connected_count: u32, best_height: u32, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx.event_tx.send(SpvEvent::PeersUpdated {
        count: connected_count,
        best_height,
    });
}

extern "C" fn on_transaction_received(
    _wallet_id: *const c_char,
    _account_index: u32,
    record: *const FFITransactionRecord,
    user_data: *mut c_void,
) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };

    if record.is_null() {
        return;
    }

    // Safety: record is a valid pointer to an FFITransactionRecord (callback contract).
    let r = unsafe { &*record };

    let (is_instant_send, is_chain_locked) = ffi_transaction_context_flags(r.context.context_type);

    let block_info = &r.context.block_info;
    let has_block = block_info.block_hash != [0u8; 32] && block_info.timestamp != 0;

    let addresses = extract_ffi_input_addresses(r);
    let inputs = extract_ffi_inputs(r);
    let outputs = extract_ffi_outputs(r, ctx.network);

    // Safety: label is a valid C string for the duration of the callback.
    let label = unsafe { extract_ffi_label(r.label) };

    let fallback_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let _ = ctx
        .event_tx
        .send(SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array(r.txid),
            amount: r.net_amount,
            direction: ffi_direction_to_direction(r.direction),
            transaction_type: ffi_type_to_type(r.transaction_type),
            timestamp: if has_block {
                block_info.timestamp as u64
            } else {
                fallback_timestamp
            },
            height: if has_block {
                Some(block_info.height)
            } else {
                None
            },
            fee: if r.fee > 0 { Some(r.fee) } else { None },
            addresses,
            block_hash: if has_block {
                Some(dashcore::BlockHash::from_byte_array(block_info.block_hash))
            } else {
                None
            },
            is_instant_send,
            is_chain_locked,
            label,
            inputs,
            outputs,
        })));
}

extern "C" fn on_balance_updated(
    _wallet_id: *const c_char,
    spendable: u64,
    unconfirmed: u64,
    immature: u64,
    locked: u64,
    user_data: *mut c_void,
) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let _ = ctx
        .event_tx
        .send(SpvEvent::BalanceUpdated(WalletCoreBalance::new(
            spendable,
            unconfirmed,
            immature,
            locked,
        )));
}

extern "C" fn on_progress_update(progress: *const FFISyncProgress, user_data: *mut c_void) {
    if progress.is_null() {
        return;
    }
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    // progress is a valid FFISyncProgress pointer provided by the FFI layer for
    // the duration of this callback.
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let rust_progress = unsafe { ffi_progress_to_rust(progress) };

    if let Ok(mut guard) = ctx.progress.write() {
        *guard = rust_progress.clone();
    }

    let _ = ctx
        .event_tx
        .send(SpvEvent::SyncProgressUpdated(Box::new(rust_progress)));
}

extern "C" fn on_client_error(error: *const c_char, user_data: *mut c_void) {
    // Safety: user_data is a valid CallbackContext pointer (borrowed, not owned).
    let ctx = unsafe { &*(user_data as *const CallbackContext) };
    let msg = if error.is_null() {
        "unknown client error".to_string()
    } else {
        // Safety: error is a borrowed C string valid for the duration of the callback.
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    };
    let _ = ctx.event_tx.send(SpvEvent::Error(msg));
}

/// Extract `(is_instant_send, is_chain_locked)` from an `FFITransactionContextType`.
fn ffi_transaction_context_flags(ctx_type: FFITransactionContextType) -> (bool, bool) {
    match ctx_type {
        FFITransactionContextType::InstantSend => (true, false),
        FFITransactionContextType::InChainLockedBlock => (false, true),
        FFITransactionContextType::Mempool | FFITransactionContextType::InBlock => (false, false),
    }
}

fn ffi_direction_to_direction(dir: FFITransactionDirection) -> TransactionDirection {
    match dir {
        FFITransactionDirection::Incoming => TransactionDirection::Incoming,
        FFITransactionDirection::Outgoing => TransactionDirection::Outgoing,
        FFITransactionDirection::Internal => TransactionDirection::Internal,
        FFITransactionDirection::CoinJoin => TransactionDirection::CoinJoin,
    }
}

fn ffi_type_to_type(tt: FFITransactionType) -> TransactionType {
    match tt {
        FFITransactionType::Standard => TransactionType::Standard,
        FFITransactionType::CoinJoin => TransactionType::CoinJoin,
        FFITransactionType::ProviderRegistration => TransactionType::ProviderRegistration,
        FFITransactionType::ProviderUpdateRegistrar => TransactionType::ProviderUpdateRegistrar,
        FFITransactionType::ProviderUpdateService => TransactionType::ProviderUpdateService,
        FFITransactionType::ProviderUpdateRevocation => TransactionType::ProviderUpdateRevocation,
        FFITransactionType::AssetLock => TransactionType::AssetLock,
        FFITransactionType::AssetUnlock => TransactionType::AssetUnlock,
        FFITransactionType::Coinbase => TransactionType::Coinbase,
        FFITransactionType::Ignored => TransactionType::Ignored,
    }
}

/// Extract an optional label string from a C string pointer.
///
/// # Safety
///
/// `ptr` must be null or point to a valid, nul-terminated C string for the
/// duration of the call.
unsafe fn extract_ffi_label(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // Safety: caller guarantees `ptr` is a valid, nul-terminated C string.
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    if s.is_empty() { None } else { Some(s) }
}

/// Extract addresses from an `FFITransactionRecord`'s input details.
fn extract_ffi_input_addresses(record: &FFITransactionRecord) -> Vec<String> {
    if record.input_details.is_null() || record.input_details_count == 0 {
        return Vec::new();
    }
    // Safety: input_details is a valid pointer to an array of input_details_count elements
    // (guaranteed by the FFI contract for the lifetime of the callback).
    let details =
        unsafe { std::slice::from_raw_parts(record.input_details, record.input_details_count) };
    let mut addrs: Vec<String> = details
        .iter()
        .filter_map(|d| {
            if d.address.is_null() {
                None
            } else {
                // Safety: address is a valid C string for the duration of the callback.
                Some(
                    unsafe { CStr::from_ptr(d.address) }
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        })
        .collect();
    addrs.sort();
    addrs.dedup();
    addrs
}

/// Extract `InputInfo` entries from an `FFITransactionRecord`.
fn extract_ffi_inputs(record: &FFITransactionRecord) -> Vec<InputInfo> {
    if record.input_details.is_null() || record.input_details_count == 0 {
        return Vec::new();
    }
    // Safety: input_details is a valid pointer to an array of input_details_count elements.
    let details =
        unsafe { std::slice::from_raw_parts(record.input_details, record.input_details_count) };
    details
        .iter()
        .map(|d| {
            let address = if d.address.is_null() {
                String::new()
            } else {
                // Safety: address is a valid C string for the duration of the callback.
                unsafe { CStr::from_ptr(d.address) }
                    .to_string_lossy()
                    .into_owned()
            };
            InputInfo {
                index: d.index,
                value: d.value,
                address,
            }
        })
        .collect()
}

/// Extract `OutputInfo` entries from an `FFITransactionRecord`.
///
/// The FFI `OutputDetail` only carries index and role. Value and address are
/// extracted from the consensus-serialized transaction bytes embedded in the
/// record.
fn extract_ffi_outputs(record: &FFITransactionRecord, network: Network) -> Vec<OutputInfo> {
    if record.output_details.is_null() || record.output_details_count == 0 {
        return Vec::new();
    }
    // Safety: output_details is a valid pointer to an array of output_details_count elements.
    let details =
        unsafe { std::slice::from_raw_parts(record.output_details, record.output_details_count) };

    // Attempt to deserialize the raw transaction so we can read output values/addresses.
    let tx: Option<dashcore::blockdata::transaction::Transaction> =
        if !record.tx_data.is_null() && record.tx_len > 0 {
            // Safety: tx_data is a valid pointer for tx_len bytes.
            let bytes = unsafe { std::slice::from_raw_parts(record.tx_data, record.tx_len) };
            dashcore::consensus::deserialize(bytes).ok()
        } else {
            None
        };

    details
        .iter()
        .map(|d| {
            let (value, address) = tx
                .as_ref()
                .and_then(|t| t.output.get(d.index as usize))
                .map(|o| {
                    let addr = dashcore::Address::from_script(&o.script_pubkey, network)
                        .ok()
                        .map_or_else(String::new, |a| a.to_string());
                    (o.value, addr)
                })
                .unwrap_or((0, String::new()));
            OutputInfo {
                index: d.index,
                value,
                address,
                role: ffi_output_role_to_role(d.role),
            }
        })
        .collect()
}

fn ffi_output_role_to_role(role: FFIOutputRole) -> OutputRole {
    match role {
        FFIOutputRole::Received => OutputRole::Received,
        FFIOutputRole::Change => OutputRole::Change,
        FFIOutputRole::Sent => OutputRole::Sent,
        FFIOutputRole::Unspendable => OutputRole::Unspendable,
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use std::ptr;

    use key_wallet_ffi::types::{FFITransactionDirection, FFITransactionType};

    use super::*;

    #[test]
    fn ffi_direction_to_direction_maps_all_variants() {
        assert_eq!(
            ffi_direction_to_direction(FFITransactionDirection::Incoming),
            TransactionDirection::Incoming,
        );
        assert_eq!(
            ffi_direction_to_direction(FFITransactionDirection::Outgoing),
            TransactionDirection::Outgoing,
        );
        assert_eq!(
            ffi_direction_to_direction(FFITransactionDirection::Internal),
            TransactionDirection::Internal,
        );
        assert_eq!(
            ffi_direction_to_direction(FFITransactionDirection::CoinJoin),
            TransactionDirection::CoinJoin,
        );
    }

    #[test]
    fn ffi_type_to_type_maps_all_variants() {
        assert_eq!(
            ffi_type_to_type(FFITransactionType::Standard),
            TransactionType::Standard,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::CoinJoin),
            TransactionType::CoinJoin,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::ProviderRegistration),
            TransactionType::ProviderRegistration,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::ProviderUpdateRegistrar),
            TransactionType::ProviderUpdateRegistrar,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::ProviderUpdateService),
            TransactionType::ProviderUpdateService,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::ProviderUpdateRevocation),
            TransactionType::ProviderUpdateRevocation,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::AssetLock),
            TransactionType::AssetLock,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::AssetUnlock),
            TransactionType::AssetUnlock,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::Coinbase),
            TransactionType::Coinbase,
        );
        assert_eq!(
            ffi_type_to_type(FFITransactionType::Ignored),
            TransactionType::Ignored,
        );
    }

    #[test]
    fn extract_ffi_label_null_returns_none() {
        let result = unsafe { extract_ffi_label(ptr::null()) };
        assert_eq!(result, None);
    }

    #[test]
    fn extract_ffi_label_empty_returns_none() {
        let s = CString::new("").unwrap();
        let result = unsafe { extract_ffi_label(s.as_ptr()) };
        assert_eq!(result, None);
    }

    #[test]
    fn extract_ffi_label_valid_returns_some() {
        let s = CString::new("coffee payment").unwrap();
        let result = unsafe { extract_ffi_label(s.as_ptr()) };
        assert_eq!(result, Some("coffee payment".to_string()));
    }
}
