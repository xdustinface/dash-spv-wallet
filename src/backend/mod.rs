use std::path::Path;

use key_wallet::wallet::managed_wallet_info::coin_selection::SelectionError;
use key_wallet::wallet::managed_wallet_info::transaction_builder::BuilderError;

use self::error::BackendError;

pub mod dispatch;
pub mod error;
pub mod events;
#[cfg(feature = "ffi")]
pub mod ffi;
pub mod mock;
pub mod native;
pub mod r#trait;
pub mod types;

fn builder_error_to_backend(e: BuilderError) -> BackendError {
    match e {
        BuilderError::InsufficientFunds {
            available,
            required,
        } => BackendError::InsufficientFunds {
            available,
            required,
        },
        BuilderError::CoinSelection(SelectionError::InsufficientFunds {
            available,
            required,
        }) => BackendError::InsufficientFunds {
            available,
            required,
        },
        other => BackendError::Internal(other.to_string()),
    }
}

/// Recursively compute the total size of a directory.
fn dir_size(dir: &Path) -> u64 {
    let mut size = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            size += dir_size(&path);
        } else {
            size += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_size_counts_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.dat"), vec![0u8; 100]).unwrap();
        std::fs::write(tmp.path().join("b.dat"), vec![0u8; 200]).unwrap();
        assert_eq!(dir_size(tmp.path()), 300);
    }

    #[test]
    fn dir_size_recurses_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/data.dat"), vec![0u8; 500]).unwrap();
        std::fs::write(tmp.path().join("top.dat"), vec![0u8; 100]).unwrap();
        assert_eq!(dir_size(tmp.path()), 600);
    }

    #[test]
    fn dir_size_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(dir_size(tmp.path()), 0);
    }

    #[test]
    fn dir_size_nonexistent_dir() {
        assert_eq!(dir_size(Path::new("/nonexistent/path")), 0);
    }
}
