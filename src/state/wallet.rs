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
                    existing.is_instant_send = *is_instant_send;
                    existing.is_chain_locked = *is_chain_locked;
                    // Update amount/addresses if this is a full event (not a status-only update)
                    if *amount != 0 {
                        existing.amount = *amount;
                        existing.addresses.clone_from(addresses);
                    }
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
                    is_instant_send: *is_instant_send,
                    is_chain_locked: *is_chain_locked,
                };

                self.transactions.insert(0, record);
            }
            _ => {}
        }
    }

    /// Replace the full transaction list (e.g., after initial load).
    pub fn set_transactions(&mut self, mut transactions: Vec<TransactionInfo>) {
        transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
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
    fn transaction_received_event_adds_to_front() {
        let mut state = WalletState::default();

        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [1u8; 32],
            amount: 100_000,
            addresses: vec!["Xaddr1".into()],
            height: None,
            timestamp: None,
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
            timestamp: None,
            is_instant_send: false,
            is_chain_locked: false,
        });
        assert_eq!(state.transactions.len(), 2);
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
}
