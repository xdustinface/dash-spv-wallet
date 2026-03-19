use std::collections::VecDeque;

use crate::backend::events::SpvEvent;

const MAX_ENTRIES: usize = 1000;

/// Filter for dev log entries by event category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCategory {
    Sync,
    Network,
    Wallet,
    Error,
}

/// A single entry in the dev mode event log.
#[derive(Debug, Clone, PartialEq)]
pub struct DevLogEntry {
    pub timestamp: u64,
    pub category: EventCategory,
    pub message: String,
}

/// Bounded event log for dev mode.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DevLog {
    entries: VecDeque<DevLogEntry>,
}

impl DevLog {
    /// Add an event to the log.
    pub fn push(&mut self, event: &SpvEvent) {
        let (category, message) = categorize_event(event);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.entries.push_back(DevLogEntry {
            timestamp,
            category,
            message,
        });

        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
    }

    /// Get all entries, optionally filtered by category.
    pub fn entries(&self, filter: Option<EventCategory>) -> Vec<&DevLogEntry> {
        match filter {
            Some(cat) => self.entries.iter().filter(|e| e.category == cat).collect(),
            None => self.entries.iter().collect(),
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

fn categorize_event(event: &SpvEvent) -> (EventCategory, String) {
    match event {
        SpvEvent::SyncProgressUpdated(p) => (
            EventCategory::Sync,
            format!("Sync progress: {:.1}% ({})", p.percentage, p.state_label()),
        ),
        SpvEvent::SyncStarted { manager } => {
            (EventCategory::Sync, format!("Sync started: {manager}"))
        }
        SpvEvent::HeadersSynced { tip_height } => (
            EventCategory::Sync,
            format!("Headers synced to height {tip_height}"),
        ),
        SpvEvent::FiltersSynced { tip_height } => (
            EventCategory::Sync,
            format!("Filters synced to height {tip_height}"),
        ),
        SpvEvent::BlockProcessed {
            height,
            new_addresses,
        } => (
            EventCategory::Sync,
            format!("Block {height} processed ({new_addresses} new addresses)"),
        ),
        SpvEvent::SyncComplete { tip_height, cycle } => (
            EventCategory::Sync,
            format!("Sync complete at height {tip_height} (cycle {cycle})"),
        ),
        SpvEvent::PeerConnected(addr) => {
            (EventCategory::Network, format!("Peer connected: {addr}"))
        }
        SpvEvent::PeerDisconnected(addr) => (
            EventCategory::Network,
            format!("Peer disconnected: {addr}"),
        ),
        SpvEvent::PeersUpdated {
            count,
            best_height,
        } => (
            EventCategory::Network,
            format!("Peers: {count}, best height: {best_height}"),
        ),
        SpvEvent::TransactionReceived {
            txid,
            amount,
            addresses,
        } => {
            let txid_short = hex::encode(&txid[..4]);
            let addr = addresses.first().map(|a| a.as_str()).unwrap_or("unknown");
            (
                EventCategory::Wallet,
                format!("Transaction {txid_short}...: {amount} sat ({addr})"),
            )
        }
        SpvEvent::BalanceUpdated(b) => (
            EventCategory::Wallet,
            format!(
                "Balance: {} confirmed, {} pending",
                b.confirmed, b.pending
            ),
        ),
        SpvEvent::ChainLockReceived { height, validated } => (
            EventCategory::Sync,
            format!("ChainLock at {height} (validated: {validated})"),
        ),
        SpvEvent::InstantLockReceived { txid, validated } => {
            let txid_short = hex::encode(&txid[..4]);
            (
                EventCategory::Sync,
                format!("InstantLock for {txid_short}... (validated: {validated})"),
            )
        }
        SpvEvent::Error(msg) => (EventCategory::Error, format!("Error: {msg}")),
    }
}

/// Helper to get a display label for sync state.
trait SyncProgressLabel {
    fn state_label(&self) -> &'static str;
}

impl SyncProgressLabel for crate::backend::types::SyncProgress {
    fn state_label(&self) -> &'static str {
        match self.state {
            crate::backend::types::SyncState::WaitForEvents => "waiting",
            crate::backend::types::SyncState::WaitingForConnections => "connecting",
            crate::backend::types::SyncState::Syncing => "syncing",
            crate::backend::types::SyncState::Synced => "synced",
            crate::backend::types::SyncState::Error => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::types::Balance;

    #[test]
    fn push_and_retrieve() {
        let mut log = DevLog::default();
        assert!(log.is_empty());

        log.push(&SpvEvent::PeerConnected("1.2.3.4".into()));
        assert_eq!(log.len(), 1);

        let entries = log.entries(None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, EventCategory::Network);
        assert!(entries[0].message.contains("1.2.3.4"));
    }

    #[test]
    fn filter_by_category() {
        let mut log = DevLog::default();
        log.push(&SpvEvent::PeerConnected("1.2.3.4".into()));
        log.push(&SpvEvent::HeadersSynced { tip_height: 1000 });
        log.push(&SpvEvent::BalanceUpdated(Balance::default()));
        log.push(&SpvEvent::Error("test error".into()));

        assert_eq!(log.entries(Some(EventCategory::Network)).len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Sync)).len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Wallet)).len(), 1);
        assert_eq!(log.entries(Some(EventCategory::Error)).len(), 1);
        assert_eq!(log.entries(None).len(), 4);
    }

    #[test]
    fn max_entries_enforced() {
        let mut log = DevLog::default();
        for i in 0..1100 {
            log.push(&SpvEvent::HeadersSynced { tip_height: i });
        }
        assert_eq!(log.len(), MAX_ENTRIES);
    }

    #[test]
    fn oldest_entries_evicted() {
        let mut log = DevLog::default();
        for i in 0..1005 {
            log.push(&SpvEvent::HeadersSynced { tip_height: i });
        }
        let entries = log.entries(None);
        // The first entry should be from i=5 (the 6th push), not i=0
        assert!(entries[0].message.contains("5"));
    }

    #[test]
    fn clear_removes_all() {
        let mut log = DevLog::default();
        log.push(&SpvEvent::PeerConnected("1.2.3.4".into()));
        log.push(&SpvEvent::PeerConnected("5.6.7.8".into()));
        assert_eq!(log.len(), 2);

        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn categorize_all_event_types() {
        let events = vec![
            (SpvEvent::PeerConnected("x".into()), EventCategory::Network),
            (
                SpvEvent::PeerDisconnected("x".into()),
                EventCategory::Network,
            ),
            (
                SpvEvent::PeersUpdated {
                    count: 1,
                    best_height: 0,
                },
                EventCategory::Network,
            ),
            (
                SpvEvent::HeadersSynced { tip_height: 0 },
                EventCategory::Sync,
            ),
            (
                SpvEvent::FiltersSynced { tip_height: 0 },
                EventCategory::Sync,
            ),
            (
                SpvEvent::BlockProcessed {
                    height: 0,
                    new_addresses: 0,
                },
                EventCategory::Sync,
            ),
            (
                SpvEvent::SyncComplete {
                    tip_height: 0,
                    cycle: 0,
                },
                EventCategory::Sync,
            ),
            (
                SpvEvent::BalanceUpdated(Balance::default()),
                EventCategory::Wallet,
            ),
            (
                SpvEvent::TransactionReceived {
                    txid: [0; 32],
                    amount: 0,
                    addresses: vec![],
                },
                EventCategory::Wallet,
            ),
            (SpvEvent::Error("e".into()), EventCategory::Error),
            (
                SpvEvent::ChainLockReceived {
                    height: 0,
                    validated: true,
                },
                EventCategory::Sync,
            ),
            (
                SpvEvent::InstantLockReceived {
                    txid: [0; 32],
                    validated: true,
                },
                EventCategory::Sync,
            ),
        ];

        for (event, expected_cat) in events {
            let (cat, _) = categorize_event(&event);
            assert_eq!(cat, expected_cat, "wrong category for {event:?}");
        }
    }
}
