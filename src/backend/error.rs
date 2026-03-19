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
