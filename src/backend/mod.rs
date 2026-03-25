use std::path::Path;

pub mod dispatch;
pub mod error;
pub mod events;
#[cfg(feature = "ffi")]
pub mod ffi;
pub mod mock;
pub mod native;
pub mod r#trait;
pub mod types;

/// Recursively compute the size of a directory, excluding a given path and its contents.
fn dir_size_excluding(dir: &Path, exclude: Option<&Path>) -> u64 {
    let mut size = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip the exact excluded path and anything inside it
        if let Some(exc) = exclude
            && (path == exc || path.starts_with(exc))
        {
            continue;
        }
        if path.is_dir() {
            size += dir_size_excluding(&path, exclude);
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
    fn dir_size_excluding_counts_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.dat"), vec![0u8; 100]).unwrap();
        std::fs::write(tmp.path().join("b.dat"), vec![0u8; 200]).unwrap();
        assert_eq!(dir_size_excluding(tmp.path(), None), 300);
    }

    #[test]
    fn dir_size_excluding_skips_excluded_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("cache")).unwrap();
        std::fs::write(tmp.path().join("cache/data.dat"), vec![0u8; 500]).unwrap();
        std::fs::create_dir(tmp.path().join("wallets")).unwrap();
        std::fs::write(tmp.path().join("wallets/wallet.mnemonic"), b"secret").unwrap();
        let size = dir_size_excluding(tmp.path(), Some(tmp.path().join("wallets").as_path()));
        assert_eq!(size, 500);
    }

    #[test]
    fn dir_size_excluding_recurses_into_ancestors_of_excluded() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("cache")).unwrap();
        std::fs::write(tmp.path().join("cache/data.dat"), vec![0u8; 500]).unwrap();
        let user_dir = tmp.path().join("user");
        std::fs::create_dir(&user_dir).unwrap();
        std::fs::write(user_dir.join("other.dat"), vec![0u8; 100]).unwrap();
        std::fs::create_dir(user_dir.join("wallets")).unwrap();
        std::fs::write(user_dir.join("wallets/wallet.mnemonic"), b"secret").unwrap();
        // Excluding a nested path should still count sibling files in the ancestor
        let size = dir_size_excluding(tmp.path(), Some(&tmp.path().join("user/wallets")));
        assert_eq!(size, 600);
    }

    #[test]
    fn dir_size_excluding_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(dir_size_excluding(tmp.path(), None), 0);
    }
}
