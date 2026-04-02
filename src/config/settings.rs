use std::path::PathBuf;
use std::{fmt, fs, io};

use clap::Parser;
use dashcore::Network;
use serde::{Deserialize, Serialize};

use super::paths;

/// Application configuration loaded from TOML file and CLI overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub network: Network,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub wallet_dir: Option<PathBuf>,
    pub dev_mode: bool,
    pub log_level: String,
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    /// Use the in-memory mock backend (CLI-only, not persisted).
    #[serde(skip)]
    pub mock_mode: bool,
    /// Backend selection (CLI-only, not persisted). "native" or "ffi".
    #[serde(skip)]
    pub backend: String,
    /// Explicit peer addresses to connect to.
    /// When non-empty, the SPV client connects exclusively to these peers.
    #[serde(default)]
    pub peers: Vec<String>,
    /// Mempool strategy: "fetch-all" or "bloom-filter".
    #[serde(default = "default_mempool_strategy")]
    pub mempool_strategy: String,
}

fn default_window_width() -> u32 {
    1024
}

fn default_window_height() -> u32 {
    768
}

fn default_mempool_strategy() -> String {
    "bloom-filter".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            network: Network::Testnet,
            data_dir: paths::default_data_dir(),
            wallet_dir: None,
            dev_mode: false,
            log_level: "info".to_string(),
            window_width: default_window_width(),
            window_height: default_window_height(),
            mock_mode: false,
            backend: "native".to_string(),
            peers: Vec::new(),
            mempool_strategy: default_mempool_strategy(),
        }
    }
}

impl AppConfig {
    /// Load configuration by merging TOML file with CLI overrides.
    ///
    /// Priority: CLI args > TOML file > defaults.
    pub fn load() -> Result<Self, ConfigError> {
        let cli = Cli::parse();

        let mut config = match fs::read_to_string(paths::config_file_path()) {
            Ok(contents) => toml::from_str::<AppConfig>(&contents)
                .map_err(|e| ConfigError::Parse(e.to_string()))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let default_config = Self::default();
                let _ = default_config.save();
                default_config
            }
            Err(e) => return Err(ConfigError::Io(e)),
        };

        // Apply CLI overrides.
        if let Some(network) = cli.network {
            config.network = network;
        }
        if cli.dev {
            config.dev_mode = true;
        }
        if let Some(log_level) = cli.log_level {
            config.log_level = log_level;
        }
        if cli.mock {
            config.mock_mode = true;
        }
        if let Some(backend) = cli.backend {
            config.backend = backend;
        }
        if !cli.peer.is_empty() {
            config.peers = cli.peer;
        }
        if let Some(mempool_strategy) = cli.mempool_strategy {
            config.mempool_strategy = mempool_strategy;
        }
        if let Some(data_dir) = cli.data_dir {
            config.data_dir = data_dir;
        }

        // Validate mempool strategy.
        let valid_strategies = ["bloom-filter", "fetch-all"];
        if !valid_strategies.contains(&config.mempool_strategy.as_str()) {
            return Err(ConfigError::Parse(format!(
                "invalid mempool_strategy '{}': must be 'bloom-filter' or 'fetch-all'",
                config.mempool_strategy
            )));
        }

        // Expand tilde in paths.
        config.data_dir = paths::expand_tilde(&config.data_dir);
        if let Some(ref dir) = config.wallet_dir {
            config.wallet_dir = Some(paths::expand_tilde(dir));
        }

        Ok(config)
    }

    /// Save the current configuration to the TOML file.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = paths::config_file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(ConfigError::Io)?;
        }
        let contents =
            toml::to_string_pretty(self).map_err(|e| ConfigError::Parse(e.to_string()))?;
        fs::write(&path, contents).map_err(ConfigError::Io)
    }

    /// Returns the resolved wallet directory.
    /// Defaults to `<data_dir>/wallets/` when `wallet_dir` is `None`.
    pub fn wallet_dir(&self) -> PathBuf {
        self.wallet_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("wallets"))
    }

    /// Create all required directories (data, wallet, config).
    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        fs::create_dir_all(&self.data_dir).map_err(ConfigError::Io)?;
        fs::create_dir_all(self.wallet_dir()).map_err(ConfigError::Io)?;
        fs::create_dir_all(paths::config_dir()).map_err(ConfigError::Io)?;
        Ok(())
    }
}

/// CLI argument parser. Fields are all optional so they only override
/// when explicitly provided.
#[derive(Parser)]
#[command(name = "dash-spv-ui", about = "Dash SPV Wallet")]
struct Cli {
    /// Select network
    #[arg(long)]
    network: Option<Network>,

    /// Enable developer mode
    #[arg(long)]
    dev: bool,

    /// Use the in-memory mock backend instead of the real SPV client
    #[arg(long)]
    mock: bool,

    /// Set log level (error, warn, info, debug, trace)
    #[arg(long)]
    log_level: Option<String>,

    /// Select backend ("native" or "ffi"). Requires --dev and the ffi feature.
    #[arg(long)]
    backend: Option<String>,

    /// Explicit peer addresses (e.g., --peer 1.2.3.4:19999)
    #[arg(long)]
    peer: Vec<String>,

    /// Mempool strategy: "fetch-all" or "bloom-filter"
    #[arg(long)]
    mempool_strategy: Option<String>,

    /// Override the data directory path
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

/// Errors that can occur during configuration loading or saving.
#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Parse(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "config I/O error: {e}"),
            Self::Parse(msg) => write!(f, "config parse error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sensible() {
        let config = AppConfig::default();
        assert_eq!(config.network, Network::Testnet);
        assert!(!config.dev_mode);
        assert_eq!(config.log_level, "info");
        assert!(config.wallet_dir.is_none());
        assert!(config.data_dir.components().count() > 0);
        assert_eq!(config.mempool_strategy, "bloom-filter");
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let config = AppConfig {
            network: Network::Mainnet,
            data_dir: PathBuf::from("/tmp/dash-test"),
            wallet_dir: Some(PathBuf::from("/tmp/dash-wallets")),
            dev_mode: true,
            log_level: "debug".to_string(),
            ..Default::default()
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(restored.network, config.network);
        assert_eq!(restored.data_dir, config.data_dir);
        assert_eq!(restored.wallet_dir, config.wallet_dir);
        assert_eq!(restored.dev_mode, config.dev_mode);
        assert_eq!(restored.log_level, config.log_level);
        assert_eq!(restored.mempool_strategy, config.mempool_strategy);
    }

    #[test]
    fn wallet_dir_defaults_to_data_dir_wallets() {
        let config = AppConfig {
            data_dir: PathBuf::from("/data"),
            wallet_dir: None,
            ..Default::default()
        };
        assert_eq!(config.wallet_dir(), PathBuf::from("/data/wallets"));
    }

    #[test]
    fn custom_wallet_dir_is_respected() {
        let config = AppConfig {
            wallet_dir: Some(PathBuf::from("/custom/wallets")),
            ..Default::default()
        };
        assert_eq!(config.wallet_dir(), PathBuf::from("/custom/wallets"));
    }

    #[test]
    fn network_serializes_as_string() {
        let config = AppConfig {
            network: Network::Regtest,
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(
            toml_str.contains("regtest"),
            "expected 'regtest' in TOML output: {toml_str}"
        );
    }

    #[test]
    fn all_networks_roundtrip() {
        for network in [
            Network::Mainnet,
            Network::Testnet,
            Network::Devnet,
            Network::Regtest,
        ] {
            let config = AppConfig {
                network,
                ..AppConfig::default()
            };
            let toml_str = toml::to_string_pretty(&config).unwrap();
            let restored: AppConfig = toml::from_str(&toml_str).unwrap();
            assert_eq!(restored.network, network);
        }
    }

    #[test]
    fn save_and_load_from_temp_dir() {
        let tmp = std::env::temp_dir().join("dash-spv-ui-test-config");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let config_path = tmp.join("config.toml");
        let config = AppConfig {
            network: Network::Regtest,
            data_dir: PathBuf::from("/tmp/data"),
            wallet_dir: Some(PathBuf::from("/tmp/wallets")),
            dev_mode: true,
            log_level: "trace".to_string(),
            ..Default::default()
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, &toml_str).unwrap();

        let restored: AppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(restored.network, Network::Regtest);
        assert!(restored.dev_mode);
        assert_eq!(restored.log_level, "trace");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn default_window_dimensions() {
        let config = AppConfig::default();
        assert_eq!(config.window_width, 1024);
        assert_eq!(config.window_height, 768);
    }

    #[test]
    fn window_dimensions_from_toml() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/tmp"
            dev_mode = false
            log_level = "info"
            window_width = 1920
            window_height = 1080
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.window_width, 1920);
        assert_eq!(config.window_height, 1080);
    }

    #[test]
    fn window_dimensions_default_when_omitted_from_toml() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/tmp"
            dev_mode = false
            log_level = "info"
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.window_width, 1024);
        assert_eq!(config.window_height, 768);
    }

    #[test]
    fn skip_fields_not_persisted_in_roundtrip() {
        let config = AppConfig {
            mock_mode: true,
            backend: "ffi".to_string(),
            ..Default::default()
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: AppConfig = toml::from_str(&toml_str).unwrap();

        assert!(!restored.mock_mode);
        assert!(restored.backend.is_empty());
    }

    #[test]
    fn peers_roundtrip_through_toml() {
        let config = AppConfig {
            peers: vec!["1.2.3.4:9999".to_string(), "5.6.7.8:19999".to_string()],
            ..Default::default()
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(restored.peers, config.peers);
    }

    #[test]
    fn peers_default_empty_when_omitted_from_toml() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/tmp"
            dev_mode = false
            log_level = "info"
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.peers.is_empty());
    }

    #[test]
    fn invalid_toml_produces_parse_error() {
        let bad_toml = "this is not valid toml {{{}}}";
        let result = toml::from_str::<AppConfig>(bad_toml);
        assert!(result.is_err());
    }

    #[test]
    fn partial_toml_uses_defaults_for_missing_fields() {
        let toml_str = r#"
            network = "mainnet"
            data_dir = "/data"
            dev_mode = false
            log_level = "warn"
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.network, Network::Mainnet);
        assert!(config.wallet_dir.is_none());
        assert_eq!(config.window_width, 1024);
        assert_eq!(config.mempool_strategy, "bloom-filter");
    }

    #[test]
    fn config_error_display() {
        let io_err = ConfigError::Io(io::Error::new(io::ErrorKind::NotFound, "gone"));
        assert!(io_err.to_string().contains("config I/O error"));

        let parse_err = ConfigError::Parse("bad value".to_string());
        assert!(parse_err.to_string().contains("config parse error"));
        assert!(parse_err.to_string().contains("bad value"));
    }

    #[test]
    fn ensure_dirs_creates_directories() {
        let tmp = std::env::temp_dir().join("dash-spv-ui-test-ensure-dirs");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = AppConfig {
            data_dir: tmp.join("data"),
            wallet_dir: Some(tmp.join("wallets")),
            ..Default::default()
        };
        config.ensure_dirs().unwrap();

        assert!(config.data_dir.exists());
        assert!(config.wallet_dir().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ensure_dirs_with_default_wallet_dir() {
        let tmp = std::env::temp_dir().join("dash-spv-ui-test-ensure-dirs-default");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = AppConfig {
            data_dir: tmp.join("data"),
            wallet_dir: None,
            ..Default::default()
        };
        config.ensure_dirs().unwrap();

        assert!(config.data_dir.exists());
        assert!(tmp.join("data").join("wallets").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_writes_valid_toml() {
        let config = AppConfig {
            network: Network::Regtest,
            data_dir: PathBuf::from("/tmp/data"),
            wallet_dir: Some(PathBuf::from("/tmp/wallets")),
            dev_mode: true,
            log_level: "debug".to_string(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        };

        config.save().unwrap();

        let path = paths::config_file_path();
        let restored: AppConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(restored.network, Network::Regtest);
        assert!(restored.dev_mode);
        assert_eq!(restored.log_level, "debug");
        assert_eq!(restored.window_width, 800);
        assert_eq!(restored.window_height, 600);
        assert_eq!(restored.wallet_dir, Some(PathBuf::from("/tmp/wallets")));
    }

    #[test]
    fn config_error_implements_std_error() {
        let io_err = ConfigError::Io(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        let std_err: &dyn std::error::Error = &io_err;
        assert!(std_err.source().is_none());

        let parse_err = ConfigError::Parse("bad".to_string());
        let std_err: &dyn std::error::Error = &parse_err;
        assert!(std_err.source().is_none());
    }

    #[test]
    fn config_error_debug() {
        let io_err = ConfigError::Io(io::Error::new(io::ErrorKind::NotFound, "missing"));
        let debug = format!("{io_err:?}");
        assert!(debug.contains("Io"));

        let parse_err = ConfigError::Parse("invalid".to_string());
        let debug = format!("{parse_err:?}");
        assert!(debug.contains("Parse"));
        assert!(debug.contains("invalid"));
    }

    #[test]
    fn default_config_backend_and_peers() {
        let config = AppConfig::default();
        assert!(!config.mock_mode);
        assert_eq!(config.backend, "native");
        assert!(config.peers.is_empty());
    }

    #[test]
    fn all_networks_serialize_distinct() {
        let networks = [
            Network::Mainnet,
            Network::Testnet,
            Network::Devnet,
            Network::Regtest,
        ];
        let mut serialized = std::collections::HashSet::new();
        for network in &networks {
            let config = AppConfig {
                network: *network,
                ..Default::default()
            };
            let toml_str = toml::to_string_pretty(&config).unwrap();
            assert!(
                serialized.insert(toml_str),
                "duplicate serialization for {network:?}"
            );
        }
    }

    #[test]
    fn devnet_roundtrip() {
        let config = AppConfig {
            network: Network::Devnet,
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.network, Network::Devnet);
    }

    #[test]
    fn mempool_strategy_roundtrip() {
        for strategy in ["bloom-filter", "fetch-all"] {
            let config = AppConfig {
                mempool_strategy: strategy.to_string(),
                ..Default::default()
            };
            let toml_str = toml::to_string_pretty(&config).unwrap();
            let restored: AppConfig = toml::from_str(&toml_str).unwrap();
            assert_eq!(restored.mempool_strategy, strategy);
        }
    }

    #[test]
    fn cli_data_dir_overrides_toml_value() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/toml/data"
            dev_mode = false
            log_level = "info"
        "#;
        let mut config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.data_dir, PathBuf::from("/toml/data"));

        // Simulate CLI override.
        let cli_data_dir = Some(PathBuf::from("/cli/data"));
        if let Some(data_dir) = cli_data_dir {
            config.data_dir = data_dir;
        }
        config.data_dir = paths::expand_tilde(&config.data_dir);

        assert_eq!(config.data_dir, PathBuf::from("/cli/data"));
    }

    #[test]
    fn cli_data_dir_tilde_expansion() {
        let mut config = AppConfig {
            data_dir: PathBuf::from("/original"),
            ..Default::default()
        };

        let cli_data_dir = Some(PathBuf::from("~/custom-dir"));
        if let Some(data_dir) = cli_data_dir {
            config.data_dir = data_dir;
        }
        config.data_dir = paths::expand_tilde(&config.data_dir);

        assert!(!config.data_dir.starts_with("~"));
        assert!(config.data_dir.ends_with("custom-dir"));
    }

    #[test]
    fn cli_data_dir_none_preserves_toml_value() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/toml/data"
            dev_mode = false
            log_level = "info"
        "#;
        let mut config: AppConfig = toml::from_str(toml_str).unwrap();

        let cli_data_dir: Option<PathBuf> = None;
        if let Some(data_dir) = cli_data_dir {
            config.data_dir = data_dir;
        }

        assert_eq!(config.data_dir, PathBuf::from("/toml/data"));
    }

    #[test]
    fn invalid_mempool_strategy_is_rejected() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/tmp"
            dev_mode = false
            log_level = "info"
            mempool_strategy = "invalid-strategy"
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mempool_strategy, "invalid-strategy");

        // Validation happens in `AppConfig::load()` which parses CLI args,
        // so we test the validation logic directly here.
        let valid_strategies = ["bloom-filter", "fetch-all"];
        assert!(!valid_strategies.contains(&config.mempool_strategy.as_str()));
    }
}
