use std::time::Duration;

use dash_spv::test_utils::TestChain;
use dash_spv_wallet::backend::error::BackendError;
use dash_spv_wallet::backend::native::NativeBackend;
use dash_spv_wallet::backend::r#trait::SpvBackend;
use dash_spv_wallet::backend::types::TransactionDirection;
use dashcore::hashes::Hash;
use dashcore::Network;

use super::helpers::{
    assert_no_duplicate_txids, assert_tx_confirmed, assert_tx_sorted, assert_tx_unconfirmed,
    collect_transaction_events, wait_for_balance_change, wait_for_peer_connected,
    wait_for_positive_balance, wait_for_sync, wait_for_sync_progress_complete,
};
use super::setup::BackendTestContext;

#[tokio::test]
async fn native_start_stop() {
    let ctx = match BackendTestContext::new(TestChain::Minimal).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    // Before wallet creation, wallet-dependent methods should return NoWallet
    assert!(
        matches!(backend.get_balance(), Err(BackendError::NoWallet)),
        "get_balance() should return NoWallet before wallet creation"
    );
    assert!(
        matches!(backend.get_transactions(), Err(BackendError::NoWallet)),
        "get_transactions() should return NoWallet before wallet creation"
    );

    // Verify network matches configuration
    assert_eq!(
        backend.network(),
        Network::Regtest,
        "Network should match configured network"
    );

    assert!(!backend.is_running());
    backend.start().await.unwrap();
    assert!(backend.is_running());
    backend.stop().await.unwrap();
    assert!(!backend.is_running());
}

#[tokio::test]
async fn native_full_sync_with_wallet() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    // Create wallet from the test mnemonic before starting sync.
    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    // Subscribe to events before starting so we don't miss any.
    let mut event_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    // Wait for sync to reach the chain tip.
    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

    // Wait a bit for wallet balance events to propagate.
    wait_for_positive_balance(&mut backend.subscribe_events(), Duration::from_secs(10)).await;

    // Verify balance matches the wallet baseline (within tolerance for rounding).
    let balance = backend.get_balance().unwrap();
    let expected_sats = (ctx.dashd.wallet.balance * 100_000_000.0) as u64;
    let tolerance = 100_000; // 0.001 DASH tolerance
    assert!(
        (balance.spendable() as i64 - expected_sats as i64).unsigned_abs() < tolerance,
        "Balance mismatch: expected ~{} sats from baseline, got {}",
        expected_sats,
        balance.spendable(),
    );

    // Verify transaction count is close to the wallet baseline. The SPV client
    // may not detect every transaction the dashd wallet knows about (e.g.,
    // self-sends that only touch change addresses outside the bloom filter).
    let txs = backend.get_transactions().unwrap();
    let expected_tx_count = ctx.dashd.wallet.unique_txid_count();
    assert_eq!(
        txs.len(),
        expected_tx_count,
        "Transaction count mismatch: expected {}, got {}",
        expected_tx_count,
        txs.len(),
    );

    // Verify chain tip height matches dashd height.
    let tip = backend.tip_height();
    assert_eq!(
        tip,
        Some(ctx.dashd.initial_height),
        "Chain tip should match dashd height"
    );

    // Verify all confirmed transactions have valid heights and timestamps.
    for tx in &txs {
        assert_tx_confirmed(tx, &format!("tx {}", tx.txid));
        // Timestamps should be reasonable (after 2020-01-01, which is 1577836800)
        assert!(
            tx.timestamp > 1_000_000_000,
            "tx {}: timestamp {} looks too small",
            tx.txid,
            tx.timestamp
        );
    }

    // Verify no duplicate txids.
    assert_no_duplicate_txids(&txs);

    // Verify transactions are sorted correctly (unconfirmed first, then by timestamp desc).
    assert_tx_sorted(&txs);

    backend.stop().await.unwrap();
    assert!(!backend.is_running());
}

#[tokio::test]
async fn native_send_transaction() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    if !ctx.dashd.supports_mining {
        eprintln!("Skipping: dashd RPC miner not available");
        return;
    }

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    let mut event_rx = backend.subscribe_events();
    backend.start().await.unwrap();

    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

    // Ensure the wallet has spendable balance after sync.
    wait_for_positive_balance(&mut backend.subscribe_events(), Duration::from_secs(10)).await;
    let initial_balance = backend.get_balance().unwrap();
    assert!(
        initial_balance.spendable() > 0,
        "Need spendable balance to send, got {}",
        initial_balance.spendable(),
    );

    // Get a recipient address from the dashd "default" wallet.
    let recipient = ctx.dashd.node.get_new_address_from_wallet("default");

    // Subscribe before sending so we catch balance changes.
    let mut balance_rx = backend.subscribe_events();

    let send_amount: u64 = 10_000_000; // 0.1 DASH

    // Verify initial balance is sufficient for send + estimated fee.
    assert!(
        initial_balance.spendable() > send_amount,
        "Initial balance {} should exceed send amount {}",
        initial_balance.spendable(),
        send_amount,
    );

    let fee_rate: u32 = 10_000;
    let txid = backend
        .send(&recipient.to_string(), send_amount, fee_rate)
        .await
        .unwrap();

    // Verify the txid is non-zero.
    assert_ne!(txid, [0u8; 32], "Returned txid should not be all zeros");

    let expected_txid = dashcore::Txid::from_byte_array(txid);

    // Verify balance decreased after sending by at least the send amount.
    let updated_balance =
        wait_for_balance_change(&mut balance_rx, &initial_balance, Duration::from_secs(30)).await;
    assert!(
        updated_balance.spendable() < initial_balance.spendable(),
        "Balance should decrease after send: before={}, after={}",
        initial_balance.spendable(),
        updated_balance.spendable(),
    );
    let decrease = initial_balance.spendable() - updated_balance.spendable();
    assert!(
        decrease >= send_amount,
        "Balance decrease ({}) should be at least the send amount ({})",
        decrease,
        send_amount,
    );

    // The wallet built this tx locally, so it should appear in get_transactions()
    // without needing a TransactionReceived event from the network.
    let txs_before_mine = backend.get_transactions().unwrap();
    let sent_tx_before = txs_before_mine.iter().find(|t| t.txid == expected_txid);
    assert!(
        sent_tx_before.is_some(),
        "Sent tx should appear in get_transactions() before mining"
    );
    let sent_tx_before = sent_tx_before.unwrap();
    assert_tx_unconfirmed(sent_tx_before, "sent tx before mining");
    assert_eq!(
        sent_tx_before.direction,
        TransactionDirection::Sent,
        "Sent tx should have Sent direction"
    );
    assert!(
        sent_tx_before.amount < 0,
        "Sent tx should have negative amount, got {}",
        sent_tx_before.amount
    );

    // Verify no duplicates in the pre-mining tx list.
    assert_no_duplicate_txids(&txs_before_mine);

    // Mine a block to confirm the transaction.
    let mining_addr = ctx.dashd.node.get_new_address_from_wallet("default");
    ctx.dashd.node.generate_blocks(1, &mining_addr);

    let new_height = ctx.dashd.initial_height + 1;
    let mut sync_rx = backend.subscribe_events();
    wait_for_sync(&mut sync_rx, new_height).await;

    // Verify the transaction is confirmed in wallet history.
    let txs = backend.get_transactions().unwrap();
    let sent_tx = txs.iter().find(|t| t.txid == expected_txid);
    assert!(
        sent_tx.is_some(),
        "Sent tx should appear in transaction history"
    );

    let sent_tx = sent_tx.unwrap();
    assert_tx_confirmed(sent_tx, "sent tx after mining");
    assert!(
        sent_tx.amount < 0,
        "Sent tx should have negative amount, got {}",
        sent_tx.amount
    );
    assert_eq!(
        sent_tx.direction,
        TransactionDirection::Sent,
        "Direction should remain Sent after confirmation"
    );

    // Verify no duplicate txids after confirmation.
    assert_no_duplicate_txids(&txs);

    // Verify sort order after confirmation.
    assert_tx_sorted(&txs);

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn native_transaction_status_lifecycle() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    if !ctx.dashd.supports_mining {
        eprintln!("Skipping: dashd RPC miner not available");
        return;
    }

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    let mut event_rx = backend.subscribe_events();
    backend.start().await.unwrap();

    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;
    wait_for_positive_balance(&mut backend.subscribe_events(), Duration::from_secs(10)).await;

    // Send a transaction from the SPV wallet.
    let recipient = ctx.dashd.node.get_new_address_from_wallet("default");
    let mut balance_rx = backend.subscribe_events();

    let send_amount: u64 = 5_000_000; // 0.05 DASH
    let initial_balance = backend.get_balance().unwrap();
    let txid = backend
        .send(&recipient.to_string(), send_amount, 10_000)
        .await
        .unwrap();

    // Wait for the balance to update, confirming the wallet processed the send.
    wait_for_balance_change(&mut balance_rx, &initial_balance, Duration::from_secs(30)).await;

    let expected_txid = dashcore::Txid::from_byte_array(txid);

    // Step 1: Verify tx is unconfirmed via get_transactions().
    let txs_unconfirmed = backend.get_transactions().unwrap();
    let tx_before = txs_unconfirmed
        .iter()
        .find(|t| t.txid == expected_txid)
        .expect("Unconfirmed tx should be in get_transactions()");
    assert_tx_unconfirmed(tx_before, "lifecycle: before mining");

    let amount_before = tx_before.amount;
    let direction_before = tx_before.direction;

    // Step 2: Mine a block to confirm.
    let mining_addr = ctx.dashd.node.get_new_address_from_wallet("default");
    ctx.dashd.node.generate_blocks(1, &mining_addr);

    let new_height = ctx.dashd.initial_height + 1;
    let mut sync_rx = backend.subscribe_events();
    wait_for_sync(&mut sync_rx, new_height).await;

    // Step 3: Verify the same tx is confirmed.
    let txs_confirmed = backend.get_transactions().unwrap();
    let tx_after = txs_confirmed
        .iter()
        .find(|t| t.txid == expected_txid)
        .expect("Confirmed tx should be in get_transactions()");
    assert_tx_confirmed(tx_after, "lifecycle: after mining");

    // Step 4: Verify the tx appears exactly once (no duplicates from status update).
    let match_count = txs_confirmed
        .iter()
        .filter(|t| t.txid == expected_txid)
        .count();
    assert_eq!(
        match_count, 1,
        "Transaction should appear exactly once, found {}",
        match_count
    );

    // Step 5: Amount and direction should be unchanged after confirmation.
    assert_eq!(
        tx_after.amount, amount_before,
        "Amount should not change after confirmation"
    );
    assert_eq!(
        tx_after.direction, direction_before,
        "Direction should not change after confirmation"
    );

    // Step 6: Confirmations should be positive with the new tip.
    assert!(
        tx_after.confirmations(new_height) > 0,
        "Confirmed tx should have positive confirmations"
    );

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn native_balance_updates_during_sync() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    let mut event_rx = backend.subscribe_events();
    let mut balance_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    // Wait for at least one BalanceUpdated event during sync.
    let got_balance =
        wait_for_positive_balance(&mut balance_rx, Duration::from_secs(60)).await;
    assert!(
        got_balance,
        "Should receive at least one BalanceUpdated event during sync"
    );

    // Wait for full sync to complete.
    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

    // After sync, verify final balance matches baseline.
    // Allow extra time for final balance events to propagate.
    wait_for_positive_balance(&mut backend.subscribe_events(), Duration::from_secs(10)).await;
    let balance = backend.get_balance().unwrap();
    let expected_sats = (ctx.dashd.wallet.balance * 100_000_000.0) as u64;
    let tolerance = 100_000;
    assert!(
        (balance.spendable() as i64 - expected_sats as i64).unsigned_abs() < tolerance,
        "Final balance mismatch: expected ~{} sats, got {}",
        expected_sats,
        balance.spendable(),
    );

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn native_peer_events() {
    let ctx = match BackendTestContext::new(TestChain::Minimal).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    let mut event_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    // Wait for a PeerConnected event.
    let got_peer = wait_for_peer_connected(&mut event_rx, Duration::from_secs(30)).await;
    assert!(got_peer, "Should receive a PeerConnected event");

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn native_sync_progress_events() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    let mut event_rx = backend.subscribe_events();
    let mut progress_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    // Wait for sync to complete.
    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

    // After sync, verify that the progress reports synced state.
    let synced = wait_for_sync_progress_complete(&mut progress_rx, Duration::from_secs(10)).await;
    assert!(
        synced,
        "Should receive a SyncProgressUpdated event with is_synced=true"
    );

    // Also verify via the sync_progress() method.
    let progress = backend.sync_progress();
    assert!(progress.is_synced(), "sync_progress() should report synced");

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn native_transaction_count_increases_during_sync() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = NativeBackend::new(config);

    // Create wallet but do not call get_transactions() first.
    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    let mut event_rx = backend.subscribe_events();
    let mut tx_event_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    // Collect TransactionReceived events during sync (with generous timeout).
    let sync_future = wait_for_sync(&mut event_rx, ctx.dashd.initial_height);
    let collect_future =
        collect_transaction_events(&mut tx_event_rx, 1, SYNC_TIMEOUT);

    // Run both concurrently -- sync must complete, and we collect tx events meanwhile.
    use dash_spv::test_utils::SYNC_TIMEOUT;
    let ((), tx_events) = tokio::join!(sync_future, collect_future);

    // We should have received at least one TransactionReceived event during sync.
    assert!(
        !tx_events.is_empty(),
        "Should receive TransactionReceived events during sync"
    );

    // After sync, verify get_transactions() count is close to the baseline.
    // Allow time for final wallet events.
    wait_for_positive_balance(&mut backend.subscribe_events(), Duration::from_secs(10)).await;
    let txs = backend.get_transactions().unwrap();
    let baseline = ctx.dashd.wallet.unique_txid_count();
    let min_expected = baseline * 9 / 10;
    assert!(
        txs.len() >= min_expected && txs.len() <= baseline,
        "Transaction count {} outside expected range [{}, {}]",
        txs.len(),
        min_expected,
        baseline,
    );

    backend.stop().await.unwrap();
}
