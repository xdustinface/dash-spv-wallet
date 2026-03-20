use std::path::{Path, PathBuf};

const APP_NAME: &str = "dash-spv-ui";

/// Returns the platform-specific config directory.
/// macOS: ~/Library/Application Support/dash-spv-ui/
/// Linux: ~/.config/dash-spv-ui/
/// Windows: %APPDATA%\dash-spv-ui\
pub(crate) fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

/// Returns the platform-specific data directory.
/// macOS: ~/Library/Application Support/dash-spv-ui/
/// Linux: ~/.local/share/dash-spv-ui/
/// Windows: %LOCALAPPDATA%\dash-spv-ui\
pub(crate) fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

/// Returns the config file path.
pub(crate) fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Expand `~` to the home directory in a path.
pub(crate) fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_ends_with_app_name() {
        let dir = config_dir();
        assert_eq!(dir.file_name().unwrap(), APP_NAME);
    }

    #[test]
    fn data_dir_ends_with_app_name() {
        let dir = default_data_dir();
        assert_eq!(dir.file_name().unwrap(), APP_NAME);
    }

    #[test]
    fn config_file_path_is_toml() {
        let path = config_file_path();
        assert_eq!(path.file_name().unwrap(), "config.toml");
    }

    #[test]
    fn expand_tilde_replaces_home() {
        let path = Path::new("~/some/dir");
        let expanded = expand_tilde(path);
        assert!(!expanded.starts_with("~"));
        assert!(expanded.ends_with("some/dir"));
    }

    #[test]
    fn expand_tilde_leaves_absolute_paths_unchanged() {
        let path = Path::new("/absolute/path");
        assert_eq!(expand_tilde(path), PathBuf::from("/absolute/path"));
    }
}
