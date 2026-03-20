use super::error::BackendResult;
use super::events::EventReceiver;
use super::mock::MockBackend;
use super::native::NativeBackend;
use super::r#trait::SpvBackend;
use super::types::{Network, SyncProgress, TransactionInfo, WalletCoreBalance};

/// Enum dispatch wrapper that delegates all `SpvBackend` calls to either
/// a real `NativeBackend` or an in-memory `MockBackend`.
pub(crate) enum Backend {
    Native(NativeBackend),
    Mock(MockBackend),
}

impl SpvBackend for Backend {
    async fn start(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.start().await,
            Self::Mock(b) => b.start().await,
        }
    }

    async fn stop(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.stop().await,
            Self::Mock(b) => b.stop().await,
        }
    }

    async fn pause(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.pause().await,
            Self::Mock(b) => b.pause().await,
        }
    }

    async fn resume(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.resume().await,
            Self::Mock(b) => b.resume().await,
        }
    }

    fn is_running(&self) -> bool {
        match self {
            Self::Native(b) => b.is_running(),
            Self::Mock(b) => b.is_running(),
        }
    }

    fn network(&self) -> Network {
        match self {
            Self::Native(b) => b.network(),
            Self::Mock(b) => b.network(),
        }
    }

    fn tip_height(&self) -> Option<u32> {
        match self {
            Self::Native(b) => b.tip_height(),
            Self::Mock(b) => b.tip_height(),
        }
    }

    fn sync_progress(&self) -> SyncProgress {
        match self {
            Self::Native(b) => b.sync_progress(),
            Self::Mock(b) => b.sync_progress(),
        }
    }

    fn generate_mnemonic(&self) -> BackendResult<String> {
        match self {
            Self::Native(b) => b.generate_mnemonic(),
            Self::Mock(b) => b.generate_mnemonic(),
        }
    }

    async fn create_wallet(&self, mnemonic: &str) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.create_wallet(mnemonic).await,
            Self::Mock(b) => b.create_wallet(mnemonic).await,
        }
    }

    async fn load_wallet(&self) -> BackendResult<bool> {
        match self {
            Self::Native(b) => b.load_wallet().await,
            Self::Mock(b) => b.load_wallet().await,
        }
    }

    fn get_receive_address(&self) -> BackendResult<String> {
        match self {
            Self::Native(b) => b.get_receive_address(),
            Self::Mock(b) => b.get_receive_address(),
        }
    }

    fn get_balance(&self) -> BackendResult<WalletCoreBalance> {
        match self {
            Self::Native(b) => b.get_balance(),
            Self::Mock(b) => b.get_balance(),
        }
    }

    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>> {
        match self {
            Self::Native(b) => b.get_transactions(),
            Self::Mock(b) => b.get_transactions(),
        }
    }

    async fn send(&self, address: &str, amount: u64) -> BackendResult<[u8; 32]> {
        match self {
            Self::Native(b) => b.send(address, amount).await,
            Self::Mock(b) => b.send(address, amount).await,
        }
    }

    fn subscribe_events(&self) -> EventReceiver {
        match self {
            Self::Native(b) => b.subscribe_events(),
            Self::Mock(b) => b.subscribe_events(),
        }
    }
}
