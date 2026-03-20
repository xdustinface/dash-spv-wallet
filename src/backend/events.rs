use tokio::sync::broadcast;

use super::types::{ManagerIdentifier, SyncProgress, WalletCoreBalance};

/// Events emitted by the SPV backend.
#[derive(Debug, Clone, PartialEq)]
pub enum SpvEvent {
    // Sync events
    SyncProgressUpdated(Box<SyncProgress>),
    SyncStarted {
        manager: ManagerIdentifier,
    },
    HeadersSynced {
        tip_height: u32,
    },
    FiltersSynced {
        tip_height: u32,
    },
    BlockProcessed {
        height: u32,
        new_addresses: u32,
    },
    SyncComplete {
        tip_height: u32,
        cycle: u32,
    },

    // Network events
    PeerConnected(String),
    PeerDisconnected(String),
    PeersUpdated {
        count: u32,
        best_height: u32,
    },

    // Wallet events
    TransactionReceived {
        txid: [u8; 32],
        amount: i64,
        addresses: Vec<String>,
    },
    BalanceUpdated(WalletCoreBalance),

    // Validation events
    ChainLockReceived {
        height: u32,
        validated: bool,
    },
    InstantLockReceived {
        txid: [u8; 32],
        validated: bool,
    },

    // Errors
    Error(String),
}

/// Channel receiver for SPV events. Supports multiple subscribers.
pub type EventReceiver = broadcast::Receiver<SpvEvent>;

/// Channel sender for SPV events.
pub type EventSender = broadcast::Sender<SpvEvent>;

/// Creates a new event channel with the given capacity.
pub fn event_channel(capacity: usize) -> (EventSender, EventReceiver) {
    broadcast::channel(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_channel_delivers_events() {
        let (tx, mut rx) = event_channel(16);

        let event = SpvEvent::PeerConnected("127.0.0.1:9999".to_string());
        tx.send(event.clone()).unwrap();

        let received = rx.try_recv().unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn event_channel_supports_multiple_subscribers() {
        let (tx, mut rx1) = event_channel(16);
        let mut rx2 = tx.subscribe();

        let event = SpvEvent::HeadersSynced { tip_height: 1000 };
        tx.send(event.clone()).unwrap();

        assert_eq!(rx1.try_recv().unwrap(), event);
        assert_eq!(rx2.try_recv().unwrap(), event);
    }
}
