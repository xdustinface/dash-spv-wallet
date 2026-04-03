use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::state::connection::ConnectionState;
use crate::state::dev_log::DevLog;
use crate::state::network::NetworkInfo;
use crate::state::wallet::WalletState;

/// Spawns a coroutine that bridges async backend events into reactive UI signals.
///
/// Call this once from a component that has all the required context signals.
/// The coroutine subscribes to the backend event channel and updates
/// `ConnectionState`, `WalletState`, `NetworkInfo`, and `DevLog`
/// on each received event.
pub fn use_event_bridge() {
    let backend = use_context::<Signal<Backend>>();
    let mut connection = use_context::<Signal<ConnectionState>>();
    let mut wallet = use_context::<Signal<WalletState>>();
    let mut network_info = use_context::<Signal<NetworkInfo>>();
    let mut dev_log = use_context::<Signal<DevLog>>();

    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        let mut rx = backend.read().subscribe_events();

        loop {
            match rx.recv().await {
                Ok(event) => {
                    connection.write().apply_event(&event);
                    wallet.write().apply_event(&event);
                    network_info.write().apply_event(&event);
                    dev_log.write().push(&event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    panic!(
                        "Event bridge lost {n} events — this is a bug. \
                         Increase the event channel capacity or reduce event volume."
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use dashcore::hashes::Hash;

    use crate::backend::events::{SpvEvent, event_channel};
    use crate::backend::types::{
        ManagerIdentifier, TransactionDirection, TransactionInfo, TransactionType,
        WalletCoreBalance,
    };
    use crate::state::connection::ConnectionState;
    use crate::state::dev_log::{DevLog, EventCategory};
    use crate::state::network::NetworkInfo;
    use crate::state::wallet::WalletState;

    /// Dispatch a single event to all state receivers, mirroring `use_event_bridge` logic.
    fn dispatch_event(
        event: &SpvEvent,
        connection: &mut ConnectionState,
        wallet: &mut WalletState,
        network_info: &mut NetworkInfo,
        dev_log: &mut DevLog,
    ) {
        connection.apply_event(event);
        wallet.apply_event(event);
        network_info.apply_event(event);
        dev_log.push(event);
    }

    fn make_state() -> (ConnectionState, WalletState, NetworkInfo, DevLog) {
        (
            ConnectionState::default(),
            WalletState::default(),
            NetworkInfo::default(),
            DevLog::default(),
        )
    }

    #[test]
    fn peer_connected_updates_connection_and_network() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::PeerConnected("1.2.3.4:9999".into());

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(conn, ConnectionState::Connecting);
        assert_eq!(net.connected_peers, 1);
        assert_eq!(wallet, WalletState::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Network)).len(), 1,);
    }

    #[test]
    fn sync_progress_updates_connection_state() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let progress = crate::backend::types::SyncProgress::default();
        let event = SpvEvent::SyncProgressUpdated(Box::new(progress));

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert!(matches!(conn, ConnectionState::Syncing(_)));
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
    }

    #[test]
    fn transaction_received_updates_wallet() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xAA; 32]),
            amount: 500_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(1000),
            fee: None,
            addresses: vec!["Xaddr1".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: true,
            label: None,
        }));

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(wallet.transactions.len(), 1);
        assert_eq!(wallet.transactions[0].amount, 500_000);
        assert!(wallet.transactions[0].is_chain_locked);
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Wallet)).len(), 1);
    }

    #[test]
    fn balance_updated_updates_wallet() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let balance = WalletCoreBalance::new(1_000_000, 50_000, 0, 0);
        let event = SpvEvent::BalanceUpdated(balance);

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(wallet.balance, balance);
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Wallet)).len(), 1);
    }

    #[test]
    fn broadcast_channel_delivers_to_dispatch() {
        let (tx, mut rx) = event_channel(16);
        let (mut conn, mut wallet, mut net, mut log) = make_state();

        let events = vec![
            SpvEvent::PeerConnected("1.2.3.4".into()),
            SpvEvent::BalanceUpdated(WalletCoreBalance::new(100, 0, 0, 0)),
            SpvEvent::HeadersSynced { tip_height: 5000 },
        ];

        for event in &events {
            tx.send(event.clone()).unwrap();
        }

        while let Ok(event) = rx.try_recv() {
            dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);
        }

        assert_eq!(conn, ConnectionState::Connecting);
        assert_eq!(wallet.balance, WalletCoreBalance::new(100, 0, 0, 0));
        assert_eq!(net.connected_peers, 1);
        assert_eq!(net.chain_tip, 5000);
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn multiple_subscribers_receive_same_events() {
        let (tx, mut rx1) = event_channel(16);
        let mut rx2 = tx.subscribe();

        let event = SpvEvent::PeerConnected("10.0.0.1".into());
        tx.send(event.clone()).unwrap();

        let (mut conn1, mut wallet1, mut net1, mut log1) = make_state();
        let (mut conn2, mut wallet2, mut net2, mut log2) = make_state();

        let e1 = rx1.try_recv().unwrap();
        dispatch_event(&e1, &mut conn1, &mut wallet1, &mut net1, &mut log1);

        let e2 = rx2.try_recv().unwrap();
        dispatch_event(&e2, &mut conn2, &mut wallet2, &mut net2, &mut log2);

        assert_eq!(conn1, conn2);
        assert_eq!(net1, net2);
        assert_eq!(log1.len(), log2.len());
    }

    #[test]
    fn sync_complete_transitions_to_synced() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::SyncComplete {
            tip_height: 5000,
            cycle: 2,
        };

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(conn, ConnectionState::Synced);
        assert_eq!(net.chain_tip, 5000);
        assert_eq!(wallet, WalletState::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
    }

    #[test]
    fn error_event_sets_error_state() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::Error("connection timed out".into());

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert!(matches!(conn, ConnectionState::Error(ref msg) if msg == "connection timed out"));
        assert_eq!(wallet, WalletState::default());
        assert_eq!(net, NetworkInfo::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Error)).len(), 1);
    }

    #[test]
    fn peer_disconnected_decrements_peer_count() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();

        dispatch_event(
            &SpvEvent::PeerConnected("1.2.3.4".into()),
            &mut conn,
            &mut wallet,
            &mut net,
            &mut log,
        );
        assert_eq!(net.connected_peers, 1);

        dispatch_event(
            &SpvEvent::PeerDisconnected("1.2.3.4".into()),
            &mut conn,
            &mut wallet,
            &mut net,
            &mut log,
        );
        assert_eq!(net.connected_peers, 0);
        assert_eq!(log.len(), 2);
        assert_eq!(log.entries(Some(EventCategory::Network)).len(), 2);
    }

    #[test]
    fn peers_updated_zero_disconnects() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();

        // First connect so we're in Connecting state
        dispatch_event(
            &SpvEvent::PeerConnected("1.2.3.4".into()),
            &mut conn,
            &mut wallet,
            &mut net,
            &mut log,
        );
        assert_eq!(conn, ConnectionState::Connecting);

        let event = SpvEvent::PeersUpdated {
            count: 0,
            best_height: 0,
        };
        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(conn, ConnectionState::Disconnected);
        assert_eq!(net.connected_peers, 0);
        assert_eq!(net.best_height, 0);
    }

    #[test]
    fn sync_started_logged_as_sync() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::SyncStarted {
            manager: ManagerIdentifier::BlockHeader,
        };

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        // SyncStarted doesn't change connection/wallet/network state
        assert_eq!(conn, ConnectionState::Disconnected);
        assert_eq!(wallet, WalletState::default());
        assert_eq!(net, NetworkInfo::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
    }

    #[test]
    fn filters_synced_logged_as_sync() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::FiltersSynced { tip_height: 3000 };

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(conn, ConnectionState::Disconnected);
        assert_eq!(wallet, WalletState::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
    }

    #[test]
    fn block_processed_logged_as_sync() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::BlockProcessed {
            height: 1500,
            new_addresses: 3,
        };

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(conn, ConnectionState::Disconnected);
        assert_eq!(wallet, WalletState::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
    }

    #[test]
    fn chain_lock_received_logged_as_sync() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::ChainLockReceived {
            height: 10000,
            validated: true,
        };

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(conn, ConnectionState::Disconnected);
        assert_eq!(wallet, WalletState::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
    }

    #[test]
    fn instant_lock_received_logged_as_sync() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();
        let event = SpvEvent::InstantLockReceived {
            txid: [0xBB; 32],
            validated: false,
        };

        dispatch_event(&event, &mut conn, &mut wallet, &mut net, &mut log);

        assert_eq!(conn, ConnectionState::Disconnected);
        assert_eq!(wallet, WalletState::default());
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
    }

    #[test]
    fn full_lifecycle_sequence() {
        let (mut conn, mut wallet, mut net, mut log) = make_state();

        let events = vec![
            SpvEvent::PeerConnected("1.2.3.4".into()),
            SpvEvent::PeerConnected("5.6.7.8".into()),
            SpvEvent::PeersUpdated {
                count: 2,
                best_height: 10000,
            },
            SpvEvent::SyncProgressUpdated(Box::default()),
            SpvEvent::TransactionReceived(Box::new(TransactionInfo {
                txid: dashcore::Txid::from_byte_array([1; 32]),
                amount: 1_000_000,
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 1700000000,
                height: Some(9999),
                fee: None,
                addresses: vec!["Xaddr".into()],
                block_hash: None,
                is_instant_send: true,
                is_chain_locked: false,
                label: None,
            })),
            SpvEvent::BalanceUpdated(WalletCoreBalance::new(1_000_000, 0, 0, 0)),
            SpvEvent::SyncComplete {
                tip_height: 10000,
                cycle: 1,
            },
        ];

        for event in &events {
            dispatch_event(event, &mut conn, &mut wallet, &mut net, &mut log);
        }

        assert_eq!(conn, ConnectionState::Synced);
        assert_eq!(net.connected_peers, 2);
        assert_eq!(net.best_height, 10000);
        assert_eq!(net.chain_tip, 10000);
        assert_eq!(wallet.transactions.len(), 1);
        assert_eq!(wallet.balance, WalletCoreBalance::new(1_000_000, 0, 0, 0));
        assert_eq!(log.len(), 7);
    }
}
