use std::cmp::Ordering;

use crate::backend::events::SpvEvent;
use crate::backend::types::{TransactionInfo, WalletCoreBalance};

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
            SpvEvent::TransactionReceived(info) => {
                // If this txid already exists, update its context fields and
                // preserve direction/type/label from the original record when
                // the incoming event is a status-only update (amount == 0).
                if let Some(existing) = self.transactions.iter_mut().find(|t| t.txid == info.txid) {
                    existing.height = info.height;
                    if info.timestamp > 0 {
                        existing.timestamp = info.timestamp;
                    }
                    existing.block_hash = info.block_hash;
                    existing.is_instant_send = info.is_instant_send;
                    existing.is_chain_locked = info.is_chain_locked;
                    if info.amount != 0 {
                        existing.amount = info.amount;
                        existing.direction = info.direction;
                        existing.transaction_type = info.transaction_type;
                        existing.addresses.clone_from(&info.addresses);
                        existing.label.clone_from(&info.label);
                    }
                    sort_transactions(&mut self.transactions);
                    return;
                }

                self.transactions.push(*info.clone());
                sort_transactions(&mut self.transactions);
            }
            _ => {}
        }
    }

    /// Update the label on a transaction in the local state.
    pub fn set_transaction_label(&mut self, txid: &dashcore::Txid, label: Option<String>) {
        if let Some(tx) = self.transactions.iter_mut().find(|t| t.txid == *txid) {
            tx.label = label;
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
    use dashcore::hashes::Hash;

    use crate::backend::types::{TransactionDirection, TransactionType};

    use super::*;

    fn tx_event(
        txid_byte: u8,
        amount: i64,
        direction: TransactionDirection,
        addresses: Vec<String>,
        height: Option<u32>,
        timestamp: u64,
    ) -> SpvEvent {
        SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([txid_byte; 32]),
            amount,
            direction,
            transaction_type: TransactionType::Standard,
            timestamp,
            height,
            fee: None,
            addresses,
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
        }))
    }

    fn status_event(
        txid_byte: u8,
        height: Option<u32>,
        timestamp: u64,
        is_instant_send: bool,
        is_chain_locked: bool,
    ) -> SpvEvent {
        SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([txid_byte; 32]),
            amount: 0,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp,
            height,
            fee: None,
            addresses: Vec::new(),
            block_hash: None,
            is_instant_send,
            is_chain_locked,
            label: None,
        }))
    }

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

        state.apply_event(&tx_event(
            1,
            100_000,
            TransactionDirection::Incoming,
            vec!["Xaddr1".into()],
            None,
            1000,
        ));
        assert_eq!(state.transactions.len(), 1);
        assert_eq!(
            state.transactions[0].direction,
            TransactionDirection::Incoming
        );

        state.apply_event(&tx_event(
            2,
            -50_000,
            TransactionDirection::Outgoing,
            vec!["Xaddr2".into()],
            None,
            2000,
        ));
        assert_eq!(state.transactions.len(), 2);
        // Newer unconfirmed tx sorts first
        assert_eq!(
            state.transactions[0].txid,
            dashcore::Txid::from_byte_array([2u8; 32])
        );
        assert_eq!(
            state.transactions[0].direction,
            TransactionDirection::Outgoing
        );
    }

    #[test]
    fn transaction_received_with_confirmation_data() {
        let mut state = WalletState::default();

        let event = SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([3u8; 32]),
            amount: 200_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(1000),
            fee: None,
            addresses: vec!["Xaddr3".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: true,
            label: None,
        }));
        state.apply_event(&event);

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

        state.apply_event(&SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([4u8; 32]),
            amount: 50_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 0,
            height: None,
            fee: None,
            addresses: vec!["Xaddr4".into()],
            block_hash: None,
            is_instant_send: true,
            is_chain_locked: false,
            label: None,
        })));

        assert_eq!(state.transactions.len(), 1);
        assert!(state.transactions[0].is_instant_send);
        assert!(!state.transactions[0].is_chain_locked);
        assert_eq!(state.transactions[0].height, None);
    }

    #[test]
    fn duplicate_txid_updates_instead_of_inserting() {
        let mut state = WalletState::default();

        // First: mempool tx
        state.apply_event(&tx_event(
            5,
            100_000,
            TransactionDirection::Incoming,
            vec!["Xaddr5".into()],
            None,
            1700000000,
        ));
        assert_eq!(state.transactions.len(), 1);
        assert_eq!(state.transactions[0].height, None);
        assert!(!state.transactions[0].is_chain_locked);

        // Second: same txid confirmed in chain-locked block (status update)
        state.apply_event(&status_event(5, Some(2000), 1700001000, false, true));

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
    fn status_update_preserves_direction_and_type() {
        let mut state = WalletState::default();

        // Original outgoing CoinJoin tx with a label
        state.apply_event(&SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([6u8; 32]),
            amount: -75_000,
            direction: TransactionDirection::CoinJoin,
            transaction_type: TransactionType::CoinJoin,
            timestamp: 1700000000,
            height: None,
            fee: Some(100),
            addresses: vec!["Xmix1".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: Some("my mix".into()),
        })));

        // Status update with hardcoded Incoming/Standard fallbacks (amount=0)
        state.apply_event(&status_event(6, Some(3000), 1700002000, false, true));

        assert_eq!(state.transactions.len(), 1);
        let tx = &state.transactions[0];
        assert_eq!(tx.direction, TransactionDirection::CoinJoin);
        assert_eq!(tx.transaction_type, TransactionType::CoinJoin);
        assert_eq!(tx.label, Some("my mix".into()));
        assert_eq!(tx.amount, -75_000);
        assert_eq!(tx.height, Some(3000));
        assert!(tx.is_chain_locked);
    }

    #[test]
    fn status_update_does_not_overwrite_timestamp_with_zero() {
        let mut state = WalletState::default();

        state.apply_event(&tx_event(
            8,
            100_000,
            TransactionDirection::Incoming,
            vec!["Xaddr8".into()],
            None,
            1700000000,
        ));
        assert_eq!(state.transactions[0].timestamp, 1700000000);

        // Status update with timestamp 0 should not overwrite
        state.apply_event(&status_event(8, Some(500), 0, false, true));

        assert_eq!(state.transactions[0].timestamp, 1700000000);
        assert_eq!(state.transactions[0].height, Some(500));
        assert!(state.transactions[0].is_chain_locked);
    }

    #[test]
    fn set_transactions_sorts_newest_first() {
        let mut state = WalletState::default();
        let txs = vec![
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([1u8; 32]),
                amount: 100,
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 1000,
                height: Some(500),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([2u8; 32]),
                amount: 200,
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 2000,
                height: Some(600),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
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
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 5000,
                height: Some(500),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([2u8; 32]),
                amount: 200,
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 1000,
                height: None,
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
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
        state.apply_event(&tx_event(
            1,
            100_000,
            TransactionDirection::Incoming,
            vec!["Xaddr1".into()],
            Some(1000),
            5000,
        ));

        // Add an unconfirmed tx with an older timestamp
        state.apply_event(&tx_event(
            2,
            50_000,
            TransactionDirection::Incoming,
            vec!["Xaddr2".into()],
            None,
            1000,
        ));

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
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 3000,
                height: Some(300),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([2u8; 32]),
                amount: 200,
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 4000,
                height: Some(400),
                fee: None,
                addresses: vec![],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
            },
        ];
        state.set_transactions(txs);
        assert_eq!(state.transactions[0].timestamp, 4000);
        assert_eq!(state.transactions[1].timestamp, 3000);

        // New unconfirmed tx arrives via event
        state.apply_event(&tx_event(
            3,
            50_000,
            TransactionDirection::Incoming,
            vec!["Xaddr3".into()],
            None,
            2000,
        ));

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
        state.apply_event(&tx_event(
            1,
            100_000,
            TransactionDirection::Incoming,
            vec!["Xaddr1".into()],
            Some(500),
            3000,
        ));

        // Unconfirmed tx (sorts first)
        state.apply_event(&tx_event(
            2,
            50_000,
            TransactionDirection::Incoming,
            vec!["Xaddr2".into()],
            None,
            4000,
        ));
        assert_eq!(state.transactions[0].height, None);

        // The unconfirmed tx gets confirmed
        state.apply_event(&status_event(2, Some(600), 4000, false, true));

        // Both confirmed now, sorted by timestamp descending
        assert_eq!(state.transactions[0].timestamp, 4000);
        assert_eq!(state.transactions[0].height, Some(600));
        assert_eq!(state.transactions[1].timestamp, 3000);
        assert_eq!(state.transactions[1].height, Some(500));
    }

    #[test]
    fn label_propagated_from_event() {
        let mut state = WalletState::default();

        state.apply_event(&SpvEvent::TransactionReceived(Box::new(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([7u8; 32]),
            amount: 42_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(500),
            fee: None,
            addresses: vec!["Xaddr7".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: Some("payment for coffee".into()),
        })));

        assert_eq!(state.transactions.len(), 1);
        assert_eq!(
            state.transactions[0].label,
            Some("payment for coffee".into())
        );
    }

    #[test]
    fn set_transaction_label_updates_existing() {
        let mut state = WalletState::default();
        let txid = dashcore::Txid::from_byte_array([9u8; 32]);

        state.apply_event(&tx_event(
            9,
            100_000,
            TransactionDirection::Incoming,
            vec!["Xaddr9".into()],
            Some(500),
            1700000000,
        ));

        state.set_transaction_label(&txid, Some("rent payment".into()));
        assert_eq!(state.transactions[0].label, Some("rent payment".into()));

        state.set_transaction_label(&txid, None);
        assert_eq!(state.transactions[0].label, None);
    }

    #[test]
    fn set_transaction_label_nonexistent_is_noop() {
        let mut state = WalletState::default();
        let txid = dashcore::Txid::from_byte_array([99u8; 32]);

        state.set_transaction_label(&txid, Some("label".into()));
        assert!(state.transactions.is_empty());
    }
}
