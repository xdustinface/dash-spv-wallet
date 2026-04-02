use dash_spv::test_utils::{DashdTestContext, TestChain, retain_test_dir};
use dash_spv_wallet::backend::events::SpvEvent;
use dash_spv_wallet::config::AppConfig;
use dashcore::Network;
use tempfile::TempDir;

/// Test infrastructure wrapping a dashd node and temporary storage directories.
pub struct BackendTestContext {
    pub dashd: DashdTestContext,
    pub storage_dir: TempDir,
}

impl BackendTestContext {
    /// Create a new test context for the given chain variant.
    ///
    /// Returns `None` if `SKIP_DASHD_TESTS` is set or if dashd fails to
    /// start. Panics if `DASHD_PATH` or `DASHD_TEST_DATA` are missing.
    pub async fn new(chain: TestChain) -> Option<Self> {
        if std::env::var("SKIP_DASHD_TESTS").is_ok() {
            eprintln!("Skipping: SKIP_DASHD_TESTS is set");
            return None;
        }

        if std::env::var("DASHD_PATH").is_err() || std::env::var("DASHD_TEST_DATA").is_err() {
            panic!(
                "DASHD_PATH and DASHD_TEST_DATA environment variables are required. \
                 Run `eval $(python3 contrib/setup-dashd.py)` to set them, \
                 or set SKIP_DASHD_TESTS=1 to skip these tests."
            );
        }

        // Spawn in a separate task so panics from dashd startup failures
        // (e.g., "Not enough file descriptors") are caught as JoinErrors.
        let result = tokio::task::spawn(async move { DashdTestContext::new(chain).await }).await;

        match result {
            Ok(Some(dashd)) => {
                let storage_dir = TempDir::new().expect("failed to create temp dir");
                Some(Self { dashd, storage_dir })
            }
            Ok(None) => {
                eprintln!("Skipping: DashdTestContext returned None");
                None
            }
            Err(join_err) => {
                eprintln!("Skipping: dashd failed to start: {join_err}");
                None
            }
        }
    }

    /// Build an `AppConfig` pointing at the temporary storage and the test dashd node.
    pub fn make_config(&self) -> AppConfig {
        let data_dir = self.storage_dir.path().join("data");
        let wallet_dir = self.storage_dir.path().join("wallets");
        std::fs::create_dir_all(&data_dir).expect("failed to create data dir");
        std::fs::create_dir_all(&wallet_dir).expect("failed to create wallet dir");

        let mut config = AppConfig {
            network: Network::Regtest,
            data_dir,
            wallet_dir: Some(wallet_dir),
            dev_mode: true,
            log_level: "debug".to_string(),
            ..Default::default()
        };
        config.network_config_mut().peers = vec![self.dashd.addr.to_string()];
        config
    }
}

impl Drop for BackendTestContext {
    fn drop(&mut self) {
        retain_test_dir(self.storage_dir.path(), "wallets");
    }
}

/// Check whether a received `SpvEvent` signals that sync has completed at
/// or above the given target height.
pub fn is_sync_complete(event: &SpvEvent, target_height: u32) -> bool {
    matches!(event, SpvEvent::SyncComplete { tip_height, .. } if *tip_height >= target_height)
}
