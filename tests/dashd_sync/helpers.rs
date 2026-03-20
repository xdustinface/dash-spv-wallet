use std::time::Duration;

use dash_spv::test_utils::SYNC_TIMEOUT;
use dash_spv_ui::backend::events::{EventReceiver, SpvEvent};

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
