use std::fmt;

/// Errors returned by `SpvBackend` operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// No wallet has been created or loaded.
    NoWallet,
    /// The wallet already exists.
    WalletAlreadyExists,
    /// Invalid mnemonic phrase.
    InvalidMnemonic(String),
    /// Invalid address format or wrong network.
    InvalidAddress(String),
    /// Insufficient funds for the transaction.
    InsufficientFunds { available: u64, required: u64 },
    /// The client is not running.
    NotRunning,
    /// The client is already running.
    AlreadyRunning,
    /// A network or sync error occurred.
    Sync(String),
    /// A storage or persistence error occurred.
    Storage(String),
    /// An internal error occurred.
    Internal(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoWallet => write!(f, "no wallet loaded"),
            Self::WalletAlreadyExists => write!(f, "wallet already exists"),
            Self::InvalidMnemonic(msg) => write!(f, "invalid mnemonic: {msg}"),
            Self::InvalidAddress(msg) => write!(f, "invalid address: {msg}"),
            Self::InsufficientFunds {
                available,
                required,
            } => {
                write!(f, "insufficient funds: have {available}, need {required}")
            }
            Self::NotRunning => write!(f, "client is not running"),
            Self::AlreadyRunning => write!(f, "client is already running"),
            Self::Sync(msg) => write!(f, "sync error: {msg}"),
            Self::Storage(msg) => write!(f, "storage error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Result type for backend operations.
pub type BackendResult<T> = Result<T, BackendError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_all_variants() {
        let cases: Vec<(BackendError, &str)> = vec![
            (BackendError::NoWallet, "no wallet loaded"),
            (BackendError::WalletAlreadyExists, "wallet already exists"),
            (
                BackendError::InvalidMnemonic("bad phrase".into()),
                "invalid mnemonic: bad phrase",
            ),
            (
                BackendError::InvalidAddress("wrong network".into()),
                "invalid address: wrong network",
            ),
            (
                BackendError::InsufficientFunds {
                    available: 100,
                    required: 500,
                },
                "insufficient funds: have 100, need 500",
            ),
            (BackendError::NotRunning, "client is not running"),
            (BackendError::AlreadyRunning, "client is already running"),
            (BackendError::Sync("timeout".into()), "sync error: timeout"),
            (
                BackendError::Storage("disk full".into()),
                "storage error: disk full",
            ),
            (
                BackendError::Internal("unexpected".into()),
                "internal error: unexpected",
            ),
        ];

        for (error, expected) in &cases {
            assert_eq!(error.to_string(), *expected, "failed for {error:?}");
        }
    }

    #[test]
    fn debug_is_implemented() {
        let error = BackendError::NoWallet;
        let debug = format!("{error:?}");
        assert!(debug.contains("NoWallet"));
    }

    #[test]
    fn implements_std_error() {
        let error = BackendError::Internal("test".into());
        let std_error: &dyn std::error::Error = &error;
        assert!(std_error.source().is_none());
        assert_eq!(std_error.to_string(), "internal error: test");
    }

    #[test]
    fn backend_result_ok_and_err() {
        let ok: BackendResult<u32> = Ok(42);
        assert!(ok.is_ok());

        let err: BackendResult<u32> = Err(BackendError::NoWallet);
        assert!(err.is_err());
    }
}
