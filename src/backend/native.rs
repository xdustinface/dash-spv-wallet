// NativeBackend implementation using dash-spv crate directly.
//
// This module requires the `native` feature and the following dependencies:
//   dash-spv = { git = "https://github.com/dashpay/rust-dashcore", branch = "v0.42-dev" }
//   key-wallet-manager = { git = "https://github.com/dashpay/rust-dashcore", branch = "v0.42-dev" }
//
// The implementation wraps `DashSpvClient` and implements `SpvBackend` by:
// - Mapping `ClientConfig::mainnet/testnet/regtest` to the configured network
// - Implementing `EventHandler` to bridge dash-spv events to `SpvEvent` channel
// - Wrapping wallet operations via `key-wallet-manager`
// - Persisting wallet state to platform data directory
//
// When the dependencies are added, uncomment and implement the struct below.

// use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
// use tokio::sync::{broadcast, RwLock};
//
// use super::error::{BackendError, BackendResult};
// use super::events::{EventReceiver, EventSender, SpvEvent, event_channel};
// use super::r#trait::SpvBackend;
// use super::types::{Balance, Network, SyncProgress, TransactionRecord};
//
// pub struct NativeBackend {
//     network: Network,
//     running: AtomicBool,
//     event_tx: EventSender,
//     // client: Option<DashSpvClient<...>>,
//     // wallet: Arc<RwLock<ManagedWalletInfo>>,
//     // data_dir: PathBuf,
// }
//
// impl NativeBackend {
//     pub fn new(network: Network, data_dir: PathBuf) -> BackendResult<Self> {
//         let (event_tx, _) = event_channel(256);
//         Ok(Self {
//             network,
//             running: AtomicBool::new(false),
//             event_tx,
//         })
//     }
// }
//
// Key mappings from dash-spv API:
//
// Lifecycle:
//   start()  → ClientConfig::{mainnet,testnet,regtest}() + DashSpvClient::new() + client.run()
//   stop()   → client.stop()
//   pause()  → client.stop() (preserve storage)
//   resume() → recreate client from persisted state
//
// Events (EventHandler trait):
//   on_sync_event(SyncEvent::BlockHeadersStored{tip})  → SpvEvent::HeadersSynced{tip}
//   on_sync_event(SyncEvent::FiltersSyncComplete{tip})  → SpvEvent::FiltersSynced{tip}
//   on_sync_event(SyncEvent::BlockProcessed{..})        → SpvEvent::BlockProcessed{..}
//   on_sync_event(SyncEvent::SyncComplete{..})          → SpvEvent::SyncComplete{..}
//   on_sync_event(SyncEvent::ChainLockReceived{..})     → SpvEvent::ChainLockReceived{..}
//   on_sync_event(SyncEvent::InstantLockReceived{..})   → SpvEvent::InstantLockReceived{..}
//   on_network_event(NetworkEvent::PeerConnected{..})   → SpvEvent::PeerConnected(..)
//   on_network_event(NetworkEvent::PeerDisconnected{..})→ SpvEvent::PeerDisconnected(..)
//   on_network_event(NetworkEvent::PeersUpdated{..})    → SpvEvent::PeersUpdated{..}
//   on_progress(SyncProgress)                           → SpvEvent::SyncProgressUpdated(..)
//   on_wallet_event(WalletEvent::TransactionReceived{..}) → SpvEvent::TransactionReceived{..}
//   on_wallet_event(WalletEvent::BalanceUpdated{..})    → SpvEvent::BalanceUpdated(..)
//   on_error(msg)                                       → SpvEvent::Error(msg)
//
// Wallet:
//   create_wallet(mnemonic)     → Wallet::from_mnemonic() + persist
//   load_wallet()               → deserialize from <data_dir>/<network>/wallet.dat
//   get_receive_address()       → managed_account receive pool next address
//   get_balance()               → WalletCoreBalance → Balance
//   get_transactions()          → wallet TransactionRecord → backend TransactionRecord
//   send(address, amount)       → build + sign tx via key-wallet, broadcast via client
