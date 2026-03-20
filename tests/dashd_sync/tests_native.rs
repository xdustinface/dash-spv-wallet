use std::time::Duration;

use dash_spv::test_utils::TestChain;
use dash_spv_ui::backend::native::NativeBackend;
use dash_spv_ui::backend::r#trait::SpvBackend;

use super::helpers::{wait_for_positive_balance, wait_for_sync};
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
