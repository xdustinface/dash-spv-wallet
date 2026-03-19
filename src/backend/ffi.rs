// FfiBackend implementation using dash-spv-ffi C functions.
//
// This module requires the `ffi` feature and the following dependency:
//   dash-spv-ffi = { git = "https://github.com/dashpay/rust-dashcore", branch = "v0.42-dev" }
//
// The implementation calls dash-spv-ffi's 39 extern "C" functions and bridges
// FFI callbacks to the SpvEvent channel. This is the only module where `unsafe`
// code is permitted.
//
// When the dependency is added, uncomment and implement the struct below.

// use std::ffi::{c_char, c_void, CStr, CString};
// use std::sync::atomic::{AtomicBool, Ordering};
//
// use super::error::{BackendError, BackendResult};
// use super::events::{EventReceiver, EventSender, SpvEvent, event_channel};
// use super::r#trait::SpvBackend;
// use super::types::{Balance, Network, SyncProgress, TransactionRecord};
//
// pub struct FfiBackend {
//     network: Network,
//     running: AtomicBool,
//     event_tx: EventSender,
//     // client: *mut FFIDashSpvClient,
//     // config: *mut FFIClientConfig,
// }
//
// impl Drop for FfiBackend {
//     fn drop(&mut self) {
//         // unsafe {
//         //     if !self.client.is_null() {
//         //         dash_spv_ffi_client_destroy(self.client);
//         //     }
//         //     if !self.config.is_null() {
//         //         dash_spv_ffi_config_destroy(self.config);
//         //     }
//         // }
//     }
// }
//
// FFI function mappings:
//
// Lifecycle:
//   start():
//     1. dash_spv_ffi_config_new(network) or _mainnet()/_testnet()
//     2. dash_spv_ffi_config_set_data_dir(path)
//     3. dash_spv_ffi_config_set_user_agent("dash-spv-ui/<version>")
//     4. dash_spv_ffi_client_new(config)
//     5. dash_spv_ffi_client_run(client, callbacks) with FFIEventCallbacks
//   stop():    dash_spv_ffi_client_stop(client)
//   pause():   dash_spv_ffi_client_stop(client) (preserve storage)
//   resume():  recreate config + client, dash_spv_ffi_client_run()
//
// Callbacks (FFIEventCallbacks):
//   FFISyncEventCallbacks:
//     on_sync_start(manager_id)           → SpvEvent::SyncStarted
//     on_block_headers_stored(tip)        → SpvEvent::HeadersSynced
//     on_filters_sync_complete(tip)       → SpvEvent::FiltersSynced
//     on_block_processed(height, hash, n) → SpvEvent::BlockProcessed
//     on_sync_complete(tip, cycle)        → SpvEvent::SyncComplete
//     on_chainlock_received(h, hash, sig, v) → SpvEvent::ChainLockReceived
//     on_instantlock_received(txid, data, v) → SpvEvent::InstantLockReceived
//     on_manager_error(id, msg)           → SpvEvent::Error
//   FFINetworkEventCallbacks:
//     on_peer_connected(addr)             → SpvEvent::PeerConnected
//     on_peer_disconnected(addr)          → SpvEvent::PeerDisconnected
//     on_peers_updated(count, best_h)     → SpvEvent::PeersUpdated
//   FFIWalletEventCallbacks:
//     on_transaction_received(...)        → SpvEvent::TransactionReceived
//     on_balance_updated(...)             → SpvEvent::BalanceUpdated
//   FFIProgressCallback:
//     on_progress(progress)               → SpvEvent::SyncProgressUpdated
//   FFIClientErrorCallback:
//     on_error(msg)                       → SpvEvent::Error
//
// Wallet (via dash_spv_ffi_client_get_wallet_manager → key-wallet-ffi):
//   create_wallet(mnemonic) → key-wallet-ffi functions
//   get_receive_address()   → key-wallet-ffi functions
//   get_balance()           → key-wallet-ffi functions
//   send(address, amount)   → build tx via wallet FFI,
//                              broadcast via dash_spv_ffi_client_broadcast_transaction()
//
// Memory management:
//   Every _new/_create has matching _destroy/_free in Drop
//   FFISyncProgress → dash_spv_ffi_sync_progress_destroy()
//   FFIWalletManager → dash_spv_ffi_wallet_manager_free()
//   FFIClientConfig → dash_spv_ffi_config_destroy()
//
// Error handling:
//   Check return codes (0 = success)
//   On error: dash_spv_ffi_get_last_error() for detail
//
// Callback user_data pattern:
//   Box<EventSender> as *mut c_void → unbox in extern "C" callback → send event
