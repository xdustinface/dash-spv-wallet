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
