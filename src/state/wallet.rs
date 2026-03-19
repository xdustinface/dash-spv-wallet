use crate::backend::events::SpvEvent;
use crate::backend::types::{Balance, TransactionRecord};

/// Wallet state tracked by the UI.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WalletState {
    pub balance: Balance,
    pub transactions: Vec<TransactionRecord>,
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
            } => {
                let direction = if *amount >= 0 {
                    crate::backend::types::TransactionDirection::Received
                } else {
                    crate::backend::types::TransactionDirection::Sent
                };

                let record = TransactionRecord {
                    txid: *txid,
                    amount: *amount,
                    direction,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    confirmations: 0,
                    addresses: addresses.clone(),
                    is_instant_send: false,
                    is_chain_locked: false,
                };

                self.transactions.insert(0, record);
            }
            _ => {}
        }
    }

    /// Replace the full transaction list (e.g., after initial load).
    pub fn set_transactions(&mut self, mut transactions: Vec<TransactionRecord>) {
        transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        self.transactions = transactions;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::types::TransactionDirection;

    #[test]
    fn default_state() {
        let state = WalletState::default();
        assert_eq!(state.balance, Balance::default());
        assert!(state.transactions.is_empty());
        assert_eq!(state.receive_address, None);
    }

    #[test]
    fn balance_updated_event() {
        let mut state = WalletState::default();
        let balance = Balance {
            confirmed: 1_000_000,
            pending: 50_000,
            immature: 0,
            locked: 0,
        };
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
        });
        assert_eq!(state.transactions.len(), 1);
        assert_eq!(state.transactions[0].direction, TransactionDirection::Received);

        state.apply_event(&SpvEvent::TransactionReceived {
            txid: [2u8; 32],
            amount: -50_000,
            addresses: vec!["Xaddr2".into()],
        });
        assert_eq!(state.transactions.len(), 2);
        assert_eq!(state.transactions[0].txid, [2u8; 32]);
        assert_eq!(state.transactions[0].direction, TransactionDirection::Sent);
    }

    #[test]
    fn set_transactions_sorts_newest_first() {
        let mut state = WalletState::default();
        let txs = vec![
            TransactionRecord {
                txid: [1u8; 32],
                amount: 100,
                direction: TransactionDirection::Received,
                timestamp: 1000,
                confirmations: 10,
                addresses: vec![],
                is_instant_send: false,
                is_chain_locked: false,
            },
            TransactionRecord {
                txid: [2u8; 32],
                amount: 200,
                direction: TransactionDirection::Received,
                timestamp: 2000,
                confirmations: 5,
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
