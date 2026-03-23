use std::time::Duration;

use dash_spv::test_utils::TestChain;
use dash_spv_wallet::backend::ffi::FfiBackend;
use dash_spv_wallet::backend::r#trait::SpvBackend;
use dashcore::Network;

use super::helpers::{
    wait_for_balance_change, wait_for_peer_connected, wait_for_positive_balance, wait_for_sync,
    wait_for_sync_progress_complete,
};
use super::setup::BackendTestContext;

#[tokio::test]
async fn ffi_start_stop() {
    let ctx = match BackendTestContext::new(TestChain::Minimal).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = FfiBackend::new(config);

    // Before starting, get_balance should fail (client not running).
    assert!(
        backend.get_balance().is_err(),
        "get_balance() should fail before starting"
    );

    // Verify network matches configuration.
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
async fn ffi_full_sync_with_wallet() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = FfiBackend::new(config);

    // Create wallet (saves mnemonic for later loading by the FFI client).
    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    // Subscribe to events before starting so we don't miss any.
    let mut event_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    // Wait for sync to reach the chain tip.
    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

    // Wait for balance to propagate through FFI callbacks.
    wait_for_positive_balance(&mut backend.subscribe_events(), Duration::from_secs(10)).await;

    // Verify balance from the backend API matches baseline.
    let balance = backend.get_balance().unwrap();
    let expected_sats = (ctx.dashd.wallet.balance * 100_000_000.0) as u64;
    let tolerance = 100_000;
    assert!(
        (balance.spendable() as i64 - expected_sats as i64).unsigned_abs() < tolerance,
        "Balance mismatch: expected ~{} sats from baseline, got {}",
        expected_sats,
        balance.spendable(),
    );

    // Verify chain tip height.
    let tip = backend.tip_height();
    assert_eq!(
        tip,
        Some(ctx.dashd.initial_height),
        "Chain tip should match dashd height"
    );

    backend.stop().await.unwrap();
    assert!(!backend.is_running());
}

#[tokio::test]
async fn ffi_send_transaction() {
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
    let backend = FfiBackend::new(config);

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

    // Verify initial balance is sufficient.
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

    // Mine a block to confirm the transaction.
    let mining_addr = ctx.dashd.node.get_new_address_from_wallet("default");
    ctx.dashd.node.generate_blocks(1, &mining_addr);

    let new_height = ctx.dashd.initial_height + 1;
    let mut sync_rx = backend.subscribe_events();
    wait_for_sync(&mut sync_rx, new_height).await;

    // After mining, verify balance via API.
    let final_balance = backend.get_balance().unwrap();
    assert!(
        final_balance.spendable() < initial_balance.spendable(),
        "Final balance should still be less than initial"
    );

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn ffi_balance_updates_during_sync() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = FfiBackend::new(config);

    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    let mut event_rx = backend.subscribe_events();
    let mut balance_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    let got_balance = wait_for_positive_balance(&mut balance_rx, Duration::from_secs(60)).await;
    assert!(
        got_balance,
        "Should receive at least one BalanceUpdated event during sync"
    );

    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

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
async fn ffi_peer_events() {
    let ctx = match BackendTestContext::new(TestChain::Minimal).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = FfiBackend::new(config);

    let mut event_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    let got_peer = wait_for_peer_connected(&mut event_rx, Duration::from_secs(30)).await;
    assert!(got_peer, "Should receive a PeerConnected event");

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn ffi_sync_progress_events() {
    let ctx = match BackendTestContext::new(TestChain::Full).await {
        Some(ctx) => ctx,
        None => {
            eprintln!("Skipping: dashd not available");
            return;
        }
    };

    let config = ctx.make_config();
    let backend = FfiBackend::new(config);

    backend
        .create_wallet(&ctx.dashd.wallet.mnemonic)
        .await
        .unwrap();

    let mut event_rx = backend.subscribe_events();
    let mut progress_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

    let synced = wait_for_sync_progress_complete(&mut progress_rx, Duration::from_secs(10)).await;
    assert!(
        synced,
        "Should receive a SyncProgressUpdated event with is_synced=true"
    );

    let progress = backend.sync_progress();
    assert!(progress.is_synced(), "sync_progress() should report synced");

    backend.stop().await.unwrap();
}
