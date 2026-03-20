use std::cmp::Ordering;

use dashcore::hashes::Hash;

use crate::backend::events::SpvEvent;
use crate::backend::types::{TransactionDirection, TransactionInfo, WalletCoreBalance};

/// Wallet state tracked by the UI.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WalletState {
    pub balance: WalletCoreBalance,
    pub transactions: Vec<TransactionInfo>,
    pub receive_address: Option<String>,
}

/// Sort transactions: unconfirmed first, then by timestamp descending.
fn sort_transactions(transactions: &mut [TransactionInfo]) {
    transactions.sort_by(|a, b| {
        let a_confirmed = a.height.is_some();
        let b_confirmed = b.height.is_some();
        match (a_confirmed, b_confirmed) {
            (false, true) => Ordering::Less,
            (true, false) => Ordering::Greater,
            _ => b.timestamp.cmp(&a.timestamp),
        }
    });
}

impl WalletState {
    /// Apply an SPV event to update wallet state.
    pub fn apply_event(&mut self, event: &SpvEvent) {
        match event {
            SpvEvent::BalanceUpdated(balance) => {
                self.balance = *balance;
            }
            SpvEvent::TransactionReceived {
                txid,
                amount,
                addresses,
                height,
                timestamp,
                block_hash,
                is_instant_send,
                is_chain_locked,
            } => {
                let txid_parsed = dashcore::Txid::from_byte_array(*txid);

                // If this txid already exists, update its status fields
                if let Some(existing) = self.transactions.iter_mut().find(|t| t.txid == txid_parsed)
                {
                    existing.height = *height;
                    if let Some(ts) = timestamp {
                        existing.timestamp = *ts;
                    }
                    if let Some(bh) = block_hash {
                        existing.block_hash =
                            Some(dashcore::BlockHash::from_byte_array(*bh));
                    }
                    existing.is_instant_send = *is_instant_send;
                    existing.is_chain_locked = *is_chain_locked;
                    // Update amount/addresses if this is a full event (not a status-only update)
                    if *amount != 0 {
                        existing.amount = *amount;
                        existing.addresses.clone_from(addresses);
                    }
                    sort_transactions(&mut self.transactions);
                    return;
                }

                let direction = if *amount >= 0 {
                    TransactionDirection::Received
                } else {
                    TransactionDirection::Sent
                };

                let fallback_timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let record = TransactionInfo {
                    txid: txid_parsed,
                    amount: *amount,
                    direction,
                    timestamp: timestamp.unwrap_or(fallback_timestamp),
                    height: *height,
                    fee: None,
                    addresses: addresses.clone(),
                    block_hash: block_hash
                        .map(dashcore::BlockHash::from_byte_array),
                    is_instant_send: *is_instant_send,
                    is_chain_locked: *is_chain_locked,
                };

                self.transactions.push(record);
                sort_transactions(&mut self.transactions);
            }
            _ => {}
        }
    }

    /// Replace the full transaction list (e.g., after initial load).
    pub fn set_transactions(&mut self, mut transactions: Vec<TransactionInfo>) {
        sort_transactions(&mut transactions);
        self.transactions = transactions;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state() {
        let state = WalletState::default();
        assert_eq!(state.balance, WalletCoreBalance::default());
        assert!(state.transactions.is_empty());
        assert_eq!(state.receive_address, None);
    }

    #[test]
    fn balance_updated_event() {
        let mut state = WalletState::default();
        let balance = WalletCoreBalance::new(1_000_000, 50_000, 0, 0);
        state.apply_event(&SpvEvent::BalanceUpdated(balance));
        assert_eq!(state.balance, balance);
    }

    #[test]
    fn transaction_received_events_sorted_by_timestamp() {
        let mut state = WalletState::default();

        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [1u8; 32],
            amount: 100_000,
            addresses: vec!["Xaddr1".into()],
            height: None,
            timestamp: Some(1000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });
        assert_eq!(state.transactions.len(), 1);
        assert_eq!(state.transactions[0].direction, TransactionDirection::Received);

        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [2u8; 32],
            amount: -50_000,
            addresses: vec!["Xaddr2".into()],
            height: None,
            timestamp: Some(2000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });
        assert_eq!(state.transactions.len(), 2);
        // Newer unconfirmed tx sorts first
        assert_eq!(
            state.transactions[0].txid,
            dashcore::Txid::from_byte_array([2u8; 32])
        );
        assert_eq!(state.transactions[0].direction, TransactionDirection::Sent);
    }

    #[test]
    fn transaction_received_with_confirmation_data() {
        let mut state = WalletState::default();

        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [3u8; 32],
            amount: 200_000,
            addresses: vec!["Xaddr3".into()],
            height: Some(1000),
            timestamp: Some(1700000000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: true,
        });

        assert_eq!(state.transactions.len(), 1);
        let tx = &state.transactions[0];
        assert_eq!(tx.height, Some(1000));
        assert_eq!(tx.timestamp, 1700000000);
        assert!(tx.is_chain_locked);
        assert!(!tx.is_instant_send);
    }

    #[test]
    fn transaction_received_instant_send() {
        let mut state = WalletState::default();

        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [4u8; 32],
            amount: 50_000,
            addresses: vec!["Xaddr4".into()],
            height: None,
            timestamp: None,
            block_hash: None,
            is_instant_send: true,
            is_chain_locked: false,
        });

        assert_eq!(state.transactions.len(), 1);
        assert!(state.transactions[0].is_instant_send);
        assert!(!state.transactions[0].is_chain_locked);
        assert_eq!(state.transactions[0].height, None);
    }

    #[test]
    fn duplicate_txid_updates_instead_of_inserting() {
        let mut state = WalletState::default();

        // First: mempool tx
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [5u8; 32],
            amount: 100_000,
            addresses: vec!["Xaddr5".into()],
            height: None,
            timestamp: None,
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });
        assert_eq!(state.transactions.len(), 1);
        assert_eq!(state.transactions[0].height, None);
        assert!(!state.transactions[0].is_chain_locked);

        // Second: same txid confirmed in chain-locked block (status update)
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [5u8; 32],
            amount: 0,
            addresses: Vec::new(),
            height: Some(2000),
            timestamp: Some(1700001000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: true,
        });

        // Should still be one transaction, not two
        assert_eq!(state.transactions.len(), 1);
        let tx = &state.transactions[0];
        assert_eq!(tx.height, Some(2000));
        assert_eq!(tx.timestamp, 1700001000);
        assert!(tx.is_chain_locked);
        // Original amount/addresses should be preserved (status update had amount=0)
        assert_eq!(tx.amount, 100_000);
        assert_eq!(tx.addresses, vec!["Xaddr5".to_string()]);
    }

    #[test]
    fn set_transactions_sorts_newest_first() {
        let mut state = WalletState::default();
        let txs = vec![
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([1u8; 32]),
                amount: 100,
                direction: TransactionDirection::Received,
                timestamp: 1000,
                height: Some(500),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([2u8; 32]),
                amount: 200,
                direction: TransactionDirection::Received,
                timestamp: 2000,
                height: Some(600),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
            },
        ];
        state.set_transactions(txs);
        assert_eq!(state.transactions[0].timestamp, 2000);
        assert_eq!(state.transactions[1].timestamp, 1000);
    }

    #[test]
    fn unrelated_events_are_ignored() {
        let mut state = WalletState::default();
        state.apply_event(&SpvEvent::PeerConnected("1.2.3.4".into()));
        state.apply_event(&SpvEvent::HeadersSynced { tip_height: 1000 });
        assert_eq!(state, WalletState::default());
    }

    #[test]
    fn unconfirmed_sorts_before_confirmed() {
        let mut state = WalletState::default();
        let txs = vec![
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([1u8; 32]),
                amount: 100,
                direction: TransactionDirection::Received,
                timestamp: 5000,
                height: Some(500),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([2u8; 32]),
                amount: 200,
                direction: TransactionDirection::Received,
                timestamp: 1000,
                height: None,
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
            },
        ];
        state.set_transactions(txs);
        // Unconfirmed tx sorts first despite having an older timestamp
        assert_eq!(state.transactions[0].height, None);
        assert_eq!(state.transactions[1].height, Some(500));
    }

    #[test]
    fn apply_event_maintains_sort_order() {
        let mut state = WalletState::default();

        // Add a confirmed tx with a recent timestamp
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [1u8; 32],
            amount: 100_000,
            addresses: vec!["Xaddr1".into()],
            height: Some(1000),
            timestamp: Some(5000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });

        // Add an unconfirmed tx with an older timestamp
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [2u8; 32],
            amount: 50_000,
            addresses: vec!["Xaddr2".into()],
            height: None,
            timestamp: Some(1000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });

        assert_eq!(state.transactions.len(), 2);
        // Unconfirmed sorts first
        assert_eq!(state.transactions[0].height, None);
        assert_eq!(state.transactions[1].height, Some(1000));
    }

    #[test]
    fn set_transactions_and_apply_event_consistent_order() {
        let mut state = WalletState::default();

        // Load initial confirmed transactions via set_transactions
        let txs = vec![
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([1u8; 32]),
                amount: 100,
                direction: TransactionDirection::Received,
                timestamp: 3000,
                height: Some(300),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([2u8; 32]),
                amount: 200,
                direction: TransactionDirection::Received,
                timestamp: 4000,
                height: Some(400),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
            },
        ];
        state.set_transactions(txs);
        assert_eq!(state.transactions[0].timestamp, 4000);
        assert_eq!(state.transactions[1].timestamp, 3000);

        // New unconfirmed tx arrives via event
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [3u8; 32],
            amount: 50_000,
            addresses: vec!["Xaddr3".into()],
            height: None,
            timestamp: Some(2000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });

        // Unconfirmed tx sorts to front despite older timestamp
        assert_eq!(state.transactions.len(), 3);
        assert_eq!(state.transactions[0].height, None);
        assert_eq!(state.transactions[0].timestamp, 2000);
        assert_eq!(state.transactions[1].timestamp, 4000);
        assert_eq!(state.transactions[2].timestamp, 3000);
    }

    #[test]
    fn confirmation_update_re_sorts_transaction() {
        let mut state = WalletState::default();

        // Confirmed tx
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [1u8; 32],
            amount: 100_000,
            addresses: vec!["Xaddr1".into()],
            height: Some(500),
            timestamp: Some(3000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });

        // Unconfirmed tx (sorts first)
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [2u8; 32],
            amount: 50_000,
            addresses: vec!["Xaddr2".into()],
            height: None,
            timestamp: Some(4000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
        });
        assert_eq!(state.transactions[0].height, None);

        // The unconfirmed tx gets confirmed
        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [2u8; 32],
            amount: 0,
            addresses: Vec::new(),
            height: Some(600),
            timestamp: Some(4000),
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: true,
        });

        // Both confirmed now, sorted by timestamp descending
        assert_eq!(state.transactions[0].timestamp, 4000);
        assert_eq!(state.transactions[0].height, Some(600));
        assert_eq!(state.transactions[1].timestamp, 3000);
        assert_eq!(state.transactions[1].height, Some(500));
    }
}
