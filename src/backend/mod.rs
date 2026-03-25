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

/// Recursively compute the size of a directory, excluding a given subdirectory.
fn dir_size_excluding(dir: &Path, exclude: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    if !dir.is_dir() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path == exclude {
            continue;
        }
        if path.is_dir() {
            total += dir_size_excluding(&path, exclude)?;
        } else {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_size_excluding_counts_files() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("__nonexistent__");
        std::fs::write(tmp.path().join("a.dat"), vec![0u8; 100]).unwrap();
        std::fs::write(tmp.path().join("b.dat"), vec![0u8; 200]).unwrap();
        assert_eq!(dir_size_excluding(tmp.path(), &nonexistent).unwrap(), 300);
    }

    #[test]
    fn dir_size_excluding_skips_excluded_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("cache")).unwrap();
        std::fs::write(tmp.path().join("cache/data.dat"), vec![0u8; 500]).unwrap();
        std::fs::create_dir(tmp.path().join("wallets")).unwrap();
        std::fs::write(tmp.path().join("wallets/wallet.mnemonic"), b"secret").unwrap();
        let size = dir_size_excluding(tmp.path(), tmp.path().join("wallets").as_path()).unwrap();
        assert_eq!(size, 500);
    }

    #[test]
    fn dir_size_excluding_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("__nonexistent__");
        assert_eq!(dir_size_excluding(tmp.path(), &nonexistent).unwrap(), 0);
    }
}
