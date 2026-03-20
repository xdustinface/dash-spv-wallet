use super::error::BackendResult;
use super::events::EventReceiver;
#[cfg(feature = "ffi")]
use super::ffi::FfiBackend;
use super::mock::MockBackend;
use super::native::NativeBackend;
use super::r#trait::SpvBackend;
use super::types::{Network, SyncProgress, TransactionInfo, WalletCoreBalance};

/// Enum dispatch wrapper that delegates all `SpvBackend` calls to either
/// a real `NativeBackend`, an in-memory `MockBackend`, or an `FfiBackend`.
pub enum Backend {
    Native(NativeBackend),
    Mock(MockBackend),
    #[cfg(feature = "ffi")]
    Ffi(FfiBackend),
}

impl SpvBackend for Backend {
    async fn start(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.start().await,
            Self::Mock(b) => b.start().await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.start().await,
        }
    }

    async fn stop(&self) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.stop().await,
            Self::Mock(b) => b.stop().await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.stop().await,
        }
    }

    fn is_running(&self) -> bool {
        match self {
            Self::Native(b) => b.is_running(),
            Self::Mock(b) => b.is_running(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.is_running(),
        }
    }

    fn network(&self) -> Network {
        match self {
            Self::Native(b) => b.network(),
            Self::Mock(b) => b.network(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.network(),
        }
    }

    fn tip_height(&self) -> Option<u32> {
        match self {
            Self::Native(b) => b.tip_height(),
            Self::Mock(b) => b.tip_height(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.tip_height(),
        }
    }

    fn sync_progress(&self) -> SyncProgress {
        match self {
            Self::Native(b) => b.sync_progress(),
            Self::Mock(b) => b.sync_progress(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.sync_progress(),
        }
    }

    fn generate_mnemonic(&self) -> BackendResult<String> {
        match self {
            Self::Native(b) => b.generate_mnemonic(),
            Self::Mock(b) => b.generate_mnemonic(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.generate_mnemonic(),
        }
    }

    async fn create_wallet(&self, mnemonic: &str) -> BackendResult<()> {
        match self {
            Self::Native(b) => b.create_wallet(mnemonic).await,
            Self::Mock(b) => b.create_wallet(mnemonic).await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.create_wallet(mnemonic).await,
        }
    }

    async fn load_wallet(&self) -> BackendResult<bool> {
        match self {
            Self::Native(b) => b.load_wallet().await,
            Self::Mock(b) => b.load_wallet().await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.load_wallet().await,
        }
    }

    fn get_receive_address(&self) -> BackendResult<String> {
        match self {
            Self::Native(b) => b.get_receive_address(),
            Self::Mock(b) => b.get_receive_address(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.get_receive_address(),
        }
    }

    fn get_balance(&self) -> BackendResult<WalletCoreBalance> {
        match self {
            Self::Native(b) => b.get_balance(),
            Self::Mock(b) => b.get_balance(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.get_balance(),
        }
    }

    fn get_transactions(&self) -> BackendResult<Vec<TransactionInfo>> {
        match self {
            Self::Native(b) => b.get_transactions(),
            Self::Mock(b) => b.get_transactions(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.get_transactions(),
        }
    }

    async fn send(&self, address: &str, amount: u64) -> BackendResult<[u8; 32]> {
        match self {
            Self::Native(b) => b.send(address, amount).await,
            Self::Mock(b) => b.send(address, amount).await,
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.send(address, amount).await,
        }
    }

    fn subscribe_events(&self) -> EventReceiver {
        match self {
            Self::Native(b) => b.subscribe_events(),
            Self::Mock(b) => b.subscribe_events(),
            #[cfg(feature = "ffi")]
            Self::Ffi(b) => b.subscribe_events(),
        }
    }
}
