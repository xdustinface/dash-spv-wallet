use std::time::Duration;

use dash_spv::test_utils::TestChain;
use dash_spv_ui::backend::events::SpvEvent;
use dash_spv_ui::backend::native::NativeBackend;
use dash_spv_ui::backend::r#trait::SpvBackend;
use dashcore::hashes::Hash;

use super::helpers::{
    wait_for_balance_change, wait_for_positive_balance, wait_for_sync, wait_for_transaction,
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
    backend.create_wallet(&ctx.dashd.wallet.mnemonic).await.unwrap();

    // Subscribe to events before starting so we don't miss any.
    let mut event_rx = backend.subscribe_events();

    backend.start().await.unwrap();

    // Wait for sync to reach the chain tip.
    wait_for_sync(&mut event_rx, ctx.dashd.initial_height).await;

    // Wait a bit for wallet balance events to propagate.
    let balance_ok = wait_for_positive_balance(
        &mut backend.subscribe_events(),
        Duration::from_secs(10),
    )
    .await;

    // Verify balance from the backend API.
    let balance = backend.get_balance().unwrap();
    assert!(
        balance.spendable() > 0 || balance_ok,
        "Expected positive spendable balance after sync, got {}",
        balance.spendable(),
    );

    // Verify transactions are available.
    let txs = backend.get_transactions().unwrap();
    assert!(
        !txs.is_empty(),
        "Expected transactions after syncing the full chain",
    );

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

    backend.create_wallet(&ctx.dashd.wallet.mnemonic).await.unwrap();

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

    // Subscribe before sending so we catch the mempool event.
    let mut tx_rx = backend.subscribe_events();
    let mut balance_rx = backend.subscribe_events();

    let send_amount: u64 = 10_000_000; // 0.1 DASH
    let fee_rate: u32 = 10_000;
    let txid = backend
        .send(&recipient.to_string(), send_amount, fee_rate)
        .await
        .unwrap();

    // Verify the transaction appears as unconfirmed in the event stream.
    let mempool_event =
        wait_for_transaction(&mut tx_rx, txid, Duration::from_secs(30)).await;
    match &mempool_event {
        SpvEvent::TransactionReceived { height, .. } => {
            assert_eq!(*height, None, "Mempool tx should have no block height");
        }
        _ => panic!("Expected TransactionReceived event"),
    }

    // Verify balance decreased after sending.
    let updated_balance =
        wait_for_balance_change(&mut balance_rx, &initial_balance, Duration::from_secs(10)).await;
    assert!(
        updated_balance.spendable() < initial_balance.spendable(),
        "Balance should decrease after send: before={}, after={}",
        initial_balance.spendable(),
        updated_balance.spendable(),
    );

    // Mine a block to confirm the transaction.
    let mining_addr = ctx.dashd.node.get_new_address_from_wallet("default");
    ctx.dashd.node.generate_blocks(1, &mining_addr);

    let new_height = ctx.dashd.initial_height + 1;
    let mut sync_rx = backend.subscribe_events();
    wait_for_sync(&mut sync_rx, new_height).await;

    // Verify the transaction is confirmed in wallet history.
    let txs = backend.get_transactions().unwrap();
    let expected_txid = dashcore::Txid::from_byte_array(txid);
    let sent_tx = txs.iter().find(|t| t.txid == expected_txid);
    assert!(sent_tx.is_some(), "Sent tx should appear in transaction history");

    let sent_tx = sent_tx.unwrap();
    assert!(sent_tx.height.is_some(), "Confirmed tx should have a block height");
    assert!(sent_tx.amount < 0, "Sent tx should have negative amount");

    backend.stop().await.unwrap();
}
