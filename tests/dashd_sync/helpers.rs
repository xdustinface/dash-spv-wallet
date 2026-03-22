use std::collections::HashSet;
use std::time::Duration;

use dash_spv::test_utils::SYNC_TIMEOUT;
use dash_spv_ui::backend::events::{EventReceiver, SpvEvent};
use dash_spv_ui::backend::types::{TransactionInfo, WalletCoreBalance};

use super::setup::is_sync_complete;

/// Wait until a `SyncComplete` event at or above `target_height` is received,
/// or panic after the default sync timeout.
pub async fn wait_for_sync(rx: &mut EventReceiver, target_height: u32) {
    let deadline = tokio::time::sleep(SYNC_TIMEOUT);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                panic!(
                    "Timeout ({:?}) waiting for sync to height {}",
                    SYNC_TIMEOUT, target_height,
                );
            }
            result = rx.recv() => {
                match result {
                    Ok(ref event) if is_sync_complete(event, target_height) => return,
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("Event receiver lagged by {n}");
                    }
                    Err(_) => panic!("Event channel closed while waiting for sync"),
                }
            }
        }
    }
}

/// Wait for a `BalanceUpdated` event where the spendable balance is positive,
/// or return `false` after the given timeout.
pub async fn wait_for_positive_balance(rx: &mut EventReceiver, timeout: Duration) -> bool {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => return false,
            result = rx.recv() => {
                match result {
                    Ok(SpvEvent::BalanceUpdated(balance)) if balance.spendable() > 0 => {
                        return true;
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return false,
                }
            }
        }
    }
}

/// Wait for a `BalanceUpdated` event where the spendable balance differs from
/// `initial_balance`, or panic on timeout. Returns the new balance.
pub async fn wait_for_balance_change(
    rx: &mut EventReceiver,
    initial_balance: &WalletCoreBalance,
    timeout: Duration,
) -> WalletCoreBalance {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                panic!(
                    "Timeout ({:?}) waiting for balance to change from {:?}",
                    timeout, initial_balance,
                );
            }
            result = rx.recv() => {
                match result {
                    Ok(SpvEvent::BalanceUpdated(ref balance))
                        if balance.spendable() != initial_balance.spendable() =>
                    {
                        return *balance;
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("Event receiver lagged by {n}");
                    }
                    Err(_) => panic!("Event channel closed while waiting for balance change"),
                }
            }
        }
    }
}

/// Wait for a `PeerConnected` event within the given timeout.
pub async fn wait_for_peer_connected(rx: &mut EventReceiver, timeout: Duration) -> bool {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => return false,
            result = rx.recv() => {
                match result {
                    Ok(SpvEvent::PeerConnected(_)) => return true,
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return false,
                }
            }
        }
    }
}

/// Wait for a `SyncProgressUpdated` event where the sync is complete,
/// within the given timeout.
pub async fn wait_for_sync_progress_complete(rx: &mut EventReceiver, timeout: Duration) -> bool {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => return false,
            result = rx.recv() => {
                match result {
                    Ok(SpvEvent::SyncProgressUpdated(ref progress)) if progress.is_synced() => {
                        return true;
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return false,
                }
            }
        }
    }
}

/// Collect `TransactionReceived` events until at least `min_count` are gathered
/// or the timeout expires.
pub async fn collect_transaction_events(
    rx: &mut EventReceiver,
    min_count: usize,
    timeout: Duration,
) -> Vec<SpvEvent> {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    let mut events = Vec::new();
    loop {
        tokio::select! {
            _ = &mut deadline => return events,
            result = rx.recv() => {
                match result {
                    Ok(ref event @ SpvEvent::TransactionReceived { .. }) => {
                        events.push(event.clone());
                        if events.len() >= min_count {
                            return events;
                        }
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return events,
                }
            }
        }
    }
}

/// Assert that a `TransactionInfo` is confirmed (has height and non-zero timestamp).
pub fn assert_tx_confirmed(tx: &TransactionInfo, label: &str) {
    assert!(tx.height.is_some(), "{label}: expected a block height");
    assert!(tx.timestamp > 0, "{label}: expected non-zero timestamp");
}

/// Assert that a `TransactionInfo` is unconfirmed (no height).
pub fn assert_tx_unconfirmed(tx: &TransactionInfo, label: &str) {
    assert!(tx.height.is_none(), "{label}: expected no block height");
}

/// Assert that no duplicate txids exist in a transaction list.
pub fn assert_no_duplicate_txids(txs: &[TransactionInfo]) {
    let mut seen = HashSet::new();
    for tx in txs {
        assert!(seen.insert(tx.txid), "Duplicate txid: {}", tx.txid);
    }
}

/// Assert that transactions are sorted correctly: unconfirmed first, then by
/// timestamp descending within each group.
pub fn assert_tx_sorted(txs: &[TransactionInfo]) {
    for window in txs.windows(2) {
        let a = &window[0];
        let b = &window[1];
        match (a.height.is_some(), b.height.is_some()) {
            (false, true) => {} // unconfirmed before confirmed -- correct
            (true, false) => panic!(
                "Confirmed tx (height={:?}) sorted before unconfirmed tx",
                a.height
            ),
            _ => {
                assert!(
                    a.timestamp >= b.timestamp,
                    "Transactions not sorted by timestamp desc within group: {} >= {} failed",
                    a.timestamp,
                    b.timestamp
                );
            }
        }
    }
}
