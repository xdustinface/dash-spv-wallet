use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{fmt, fs, io};

use clap::Parser;
use dashcore::Network;
use serde::{Deserialize, Serialize};

use super::paths;

/// Known network variants and their TOML section key names.
/// Single source of truth for the network-to-key mapping.
const NETWORK_ENTRIES: &[(Network, &str)] = &[
    (Network::Mainnet, "mainnet"),
    (Network::Testnet, "testnet"),
    (Network::Devnet, "devnet"),
    (Network::Regtest, "regtest"),
];

/// Per-network configuration for settings that vary by network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkConfig {
    /// Explicit peer addresses to connect to.
    /// When non-empty, the SPV client connects exclusively to these peers.
    #[serde(default)]
    pub peers: Vec<String>,
    /// Mempool strategy: "fetch-all" or "bloom-filter".
    #[serde(default = "default_mempool_strategy")]
    pub mempool_strategy: String,
}

/// Application configuration loaded from TOML file and CLI overrides.
///
/// Serialization and deserialization are handled manually so that
/// per-network sections (e.g., `[testnet]`) appear as top-level TOML
/// tables rather than nested under a `networks` key.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub network: Network,
    pub data_dir: PathBuf,
    pub wallet_dir: Option<PathBuf>,
    pub dev_mode: bool,
    pub log_level: String,
    pub window_width: u32,
    pub window_height: u32,
    /// Use the in-memory mock backend (CLI-only, not persisted).
    pub mock_mode: bool,
    /// Backend selection (CLI-only, not persisted). "native" or "ffi".
    pub backend: String,
    /// Per-network configuration sections (e.g., `[testnet]`, `[mainnet]`).
    pub(crate) networks: BTreeMap<String, NetworkConfig>,
}

/// Helper struct for serde that handles only the flat (non-network) fields.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppConfigFlat {
    network: Network,
    data_dir: PathBuf,
    #[serde(default)]
    wallet_dir: Option<PathBuf>,
    dev_mode: bool,
    log_level: String,
    #[serde(default = "default_window_width")]
    window_width: u32,
    #[serde(default = "default_window_height")]
    window_height: u32,
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

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            peers: Vec::new(),
            mempool_strategy: default_mempool_strategy(),
        }
    }
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
            networks: BTreeMap::new(),
        }
    }
}

impl Serialize for AppConfig {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let flat = AppConfigFlat {
            network: self.network,
            data_dir: self.data_dir.clone(),
            wallet_dir: self.wallet_dir.clone(),
            dev_mode: self.dev_mode,
            log_level: self.log_level.clone(),
            window_width: self.window_width,
            window_height: self.window_height,
        };

        let mut flat_value = toml::Value::try_from(&flat).map_err(serde::ser::Error::custom)?;
        let table = flat_value
            .as_table_mut()
            .ok_or_else(|| serde::ser::Error::custom("expected table"))?;

        for (key, net_cfg) in &self.networks {
            let val = toml::Value::try_from(net_cfg).map_err(serde::ser::Error::custom)?;
            table.insert(key.clone(), val);
        }

        let mut map = serializer.serialize_map(Some(table.len()))?;
        for (k, v) in table {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for AppConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut table = toml::Table::deserialize(deserializer)?;

        // Extract network sections before deserializing the flat fields.
        let mut networks = BTreeMap::new();
        for &(_, key) in NETWORK_ENTRIES {
            if let Some(val) = table.remove(key) {
                let net_cfg: NetworkConfig = val.try_into().map_err(serde::de::Error::custom)?;
                networks.insert(key.to_string(), net_cfg);
            }
        }

        let flat: AppConfigFlat = toml::Value::Table(table)
            .try_into()
            .map_err(serde::de::Error::custom)?;

        Ok(Self {
            network: flat.network,
            data_dir: flat.data_dir,
            wallet_dir: flat.wallet_dir,
            dev_mode: flat.dev_mode,
            log_level: flat.log_level,
            window_width: flat.window_width,
            window_height: flat.window_height,
            mock_mode: false,
            backend: String::new(),
            networks,
        })
    }
}

impl AppConfig {
    /// Returns the TOML section key for the given network.
    pub(crate) fn network_key(network: Network) -> &'static str {
        NETWORK_ENTRIES
            .iter()
            .find(|(n, _)| *n == network)
            .map(|(_, key)| *key)
            .unwrap_or("devnet")
    }

    /// Returns the active network's configuration.
    pub fn network_config(&self) -> &NetworkConfig {
        static DEFAULT: LazyLock<NetworkConfig> = LazyLock::new(NetworkConfig::default);
        self.networks
            .get(Self::network_key(self.network))
            .unwrap_or(&DEFAULT)
    }

    /// Returns a mutable reference to the active network's configuration,
    /// creating a default entry if none exists.
    pub fn network_config_mut(&mut self) -> &mut NetworkConfig {
        let key = Self::network_key(self.network).to_string();
        self.networks.entry(key).or_default()
    }

    /// Returns the peer list for the active network.
    pub fn peers(&self) -> &[String] {
        &self.network_config().peers
    }

    /// Returns the mempool strategy for the active network,
    /// falling back to the default when the section is absent.
    pub fn mempool_strategy(&self) -> &str {
        let strategy = &self.network_config().mempool_strategy;
        if strategy.is_empty() {
            "bloom-filter"
        } else {
            strategy
        }
    }

    /// Load configuration by merging TOML file with CLI overrides.
    ///
    /// Priority: CLI args > TOML file > defaults.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_with_cli(Cli::parse(), &paths::config_file_path())
    }

    /// Returns the default production config file path.
    pub(crate) fn default_config_path() -> PathBuf {
        paths::config_file_path()
    }

    /// Load configuration from `config_path`, applying CLI overrides from `cli`.
    fn load_with_cli(cli: Cli, config_path: &std::path::Path) -> Result<Self, ConfigError> {
        let mut config = match fs::read_to_string(config_path) {
            Ok(contents) => toml::from_str(&contents)
                .map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let default_config = Self::default();
                let _ = default_config.save(config_path);
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
            config.network_config_mut().peers = cli.peer;
        }
        if let Some(mempool_strategy) = cli.mempool_strategy {
            config.network_config_mut().mempool_strategy = mempool_strategy;
        }
        if let Some(data_dir) = cli.data_dir {
            config.data_dir = data_dir;
        }

        // Validate mempool strategy for the active network.
        let valid_strategies = ["bloom-filter", "fetch-all"];
        let strategy = config.mempool_strategy().to_string();
        if !valid_strategies.contains(&strategy.as_str()) {
            return Err(ConfigError::Parse(format!(
                "invalid mempool_strategy '{strategy}': must be 'bloom-filter' or 'fetch-all'",
            )));
        }

        // Expand tilde in paths.
        config.data_dir = paths::expand_tilde(&config.data_dir);
        if let Some(ref dir) = config.wallet_dir {
            config.wallet_dir = Some(paths::expand_tilde(dir));
        }

        Ok(config)
    }

    /// Save the current configuration to `path`.
    pub(crate) fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(ConfigError::Io)?;
        }
        let contents =
            toml::to_string_pretty(self).map_err(|e| ConfigError::Parse(e.to_string()))?;
        fs::write(path, contents).map_err(ConfigError::Io)
    }

    /// Returns the resolved wallet directory.
    /// Defaults to `<data_dir>/wallets/` when `wallet_dir` is `None`.
    pub fn wallet_dir(&self) -> PathBuf {
        self.wallet_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("wallets"))
    }

    /// Returns the network-specific data directory (e.g., `<data_dir>/testnet/`).
    pub fn network_data_dir(&self) -> PathBuf {
        self.data_dir.join(self.network_dir_name())
    }

    fn network_dir_name(&self) -> &str {
        match self.network {
            Network::Mainnet => "mainnet",
            Network::Testnet => "testnet",
            Network::Regtest => "regtest",
            Network::Devnet => "devnet",
        }
    }

    /// Remove and recreate the network-specific data directory.
    ///
    /// Returns an error if the wallet directory is inside the network data directory
    /// to prevent accidental deletion of wallet data.
    pub(crate) fn clear_network_data_dir(&self) -> Result<(), ConfigError> {
        let network_dir = self.network_data_dir();
        let wallet_dir = self.wallet_dir();
        if wallet_dir.starts_with(&network_dir) {
            return Err(ConfigError::Parse(
                "wallet dir is inside network data dir; refusing to clear cache".to_string(),
            ));
        }
        if network_dir.exists() {
            fs::remove_dir_all(&network_dir).map_err(ConfigError::Io)?;
        }
        fs::create_dir_all(&network_dir).map_err(ConfigError::Io)?;
        Ok(())
    }

    /// Create all required directories (data, network-specific, wallet, config).
    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        fs::create_dir_all(&self.data_dir).map_err(ConfigError::Io)?;
        fs::create_dir_all(self.network_data_dir()).map_err(ConfigError::Io)?;
        fs::create_dir_all(self.wallet_dir()).map_err(ConfigError::Io)?;
        fs::create_dir_all(paths::config_dir()).map_err(ConfigError::Io)?;
        Ok(())
    }
}

/// CLI argument parser. Fields are all optional so they only override
/// when explicitly provided.
#[derive(Parser, Default)]
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
        assert_eq!(config.mempool_strategy(), "bloom-filter");
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
        assert_eq!(restored.mempool_strategy(), config.mempool_strategy());
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
        let mut config = AppConfig::default();
        config.network_config_mut().peers =
            vec!["1.2.3.4:9999".to_string(), "5.6.7.8:19999".to_string()];

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(restored.peers(), config.peers());
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
        assert!(config.peers().is_empty());
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
        assert_eq!(config.mempool_strategy(), "bloom-filter");
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
    fn network_data_dir_uses_network_name() {
        let config = AppConfig {
            data_dir: PathBuf::from("/data"),
            network: Network::Testnet,
            ..Default::default()
        };
        assert_eq!(config.network_data_dir(), PathBuf::from("/data/testnet"));

        let config = AppConfig {
            network: Network::Mainnet,
            ..config
        };
        assert_eq!(config.network_data_dir(), PathBuf::from("/data/mainnet"));

        let config = AppConfig {
            network: Network::Regtest,
            ..config
        };
        assert_eq!(config.network_data_dir(), PathBuf::from("/data/regtest"));

        let config = AppConfig {
            network: Network::Devnet,
            ..config
        };
        assert_eq!(config.network_data_dir(), PathBuf::from("/data/devnet"));
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
        assert!(config.network_data_dir().exists());
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
        assert!(config.network_data_dir().exists());
        assert!(tmp.join("data").join("wallets").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_writes_valid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");

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

        config.save(&config_path).unwrap();

        let restored: AppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(restored.network, Network::Regtest);
        assert!(restored.dev_mode);
        assert_eq!(restored.log_level, "debug");
        assert_eq!(restored.window_width, 800);
        assert_eq!(restored.window_height, 600);
        assert_eq!(restored.wallet_dir, Some(PathBuf::from("/tmp/wallets")));
    }

    #[test]
    fn load_with_cli_first_run_writes_to_specified_path() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("nonexistent.toml");
        assert!(!config_path.exists(), "precondition: path must not exist");

        let cli = Cli::default();
        let config = AppConfig::load_with_cli(cli, &config_path).unwrap();

        assert!(
            config_path.exists(),
            "load_with_cli must write defaults to the given path"
        );
        let contents = std::fs::read_to_string(&config_path).unwrap();
        let on_disk: AppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(on_disk.network, config.network);
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
        assert!(config.peers().is_empty());
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
            let mut config = AppConfig::default();
            config.network_config_mut().mempool_strategy = strategy.to_string();

            let toml_str = toml::to_string_pretty(&config).unwrap();
            let restored: AppConfig = toml::from_str(&toml_str).unwrap();
            assert_eq!(restored.mempool_strategy(), strategy);
        }
    }

    /// Write TOML content to a temporary config file and return its path.
    fn write_temp_config(name: &str, toml_str: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dash-spv-ui-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, toml_str).unwrap();
        path
    }

    #[test]
    fn cli_data_dir_overrides_toml_value() {
        let config_path = write_temp_config(
            "cli-override",
            r#"
                network = "testnet"
                data_dir = "/toml/data"
                dev_mode = false
                log_level = "info"
            "#,
        );
        let cli = Cli {
            data_dir: Some(PathBuf::from("/cli/data")),
            ..Default::default()
        };

        let config = AppConfig::load_with_cli(cli, &config_path).unwrap();
        assert_eq!(config.data_dir, PathBuf::from("/cli/data"));

        let _ = std::fs::remove_dir_all(config_path.parent().unwrap());
    }

    #[test]
    fn cli_data_dir_tilde_expansion() {
        let config_path = write_temp_config(
            "cli-tilde",
            r#"
                network = "testnet"
                data_dir = "/original"
                dev_mode = false
                log_level = "info"
            "#,
        );
        let cli = Cli {
            data_dir: Some(PathBuf::from("~/custom-dir")),
            ..Default::default()
        };

        let config = AppConfig::load_with_cli(cli, &config_path).unwrap();
        assert!(!config.data_dir.starts_with("~"));
        assert!(config.data_dir.ends_with("custom-dir"));

        let _ = std::fs::remove_dir_all(config_path.parent().unwrap());
    }

    #[test]
    fn toml_data_dir_tilde_expansion() {
        let config_path = write_temp_config(
            "toml-tilde",
            r#"
                network = "testnet"
                data_dir = "~/some/path"
                dev_mode = false
                log_level = "info"
            "#,
        );
        let cli = Cli::default();

        let config = AppConfig::load_with_cli(cli, &config_path).unwrap();
        assert!(!config.data_dir.starts_with("~"));
        assert!(config.data_dir.ends_with("some/path"));

        let _ = std::fs::remove_dir_all(config_path.parent().unwrap());
    }

    #[test]
    fn cli_data_dir_none_preserves_toml_value() {
        let config_path = write_temp_config(
            "cli-none",
            r#"
                network = "testnet"
                data_dir = "/toml/data"
                dev_mode = false
                log_level = "info"
            "#,
        );
        let cli = Cli::default();

        let config = AppConfig::load_with_cli(cli, &config_path).unwrap();
        assert_eq!(config.data_dir, PathBuf::from("/toml/data"));

        let _ = std::fs::remove_dir_all(config_path.parent().unwrap());
    }

    #[test]
    fn clear_network_data_dir_removes_and_recreates() {
        let tmp = tempfile::tempdir().unwrap();
        let network_dir = tmp.path().join("testnet");
        fs::create_dir_all(&network_dir).unwrap();
        fs::write(network_dir.join("some_file.dat"), b"data").unwrap();

        let config = AppConfig {
            data_dir: tmp.path().to_path_buf(),
            network: Network::Testnet,
            wallet_dir: Some(tmp.path().join("wallets")),
            ..Default::default()
        };

        config.clear_network_data_dir().unwrap();

        assert!(network_dir.exists(), "network dir should be recreated");
        assert!(
            !network_dir.join("some_file.dat").exists(),
            "old contents should be removed"
        );
        assert!(
            fs::read_dir(&network_dir).unwrap().next().is_none(),
            "recreated dir should be empty"
        );
    }

    #[test]
    fn clear_network_data_dir_when_dir_does_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let network_dir = tmp.path().join("testnet");
        assert!(!network_dir.exists());

        let config = AppConfig {
            data_dir: tmp.path().to_path_buf(),
            network: Network::Testnet,
            wallet_dir: Some(tmp.path().join("wallets")),
            ..Default::default()
        };

        config.clear_network_data_dir().unwrap();

        assert!(network_dir.exists(), "network dir should be created");
    }

    #[test]
    fn clear_network_data_dir_rejects_wallet_inside_network_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let network_dir = tmp.path().join("testnet");
        fs::create_dir_all(&network_dir).unwrap();

        let config = AppConfig {
            data_dir: tmp.path().to_path_buf(),
            network: Network::Testnet,
            wallet_dir: Some(network_dir.join("wallets")),
            ..Default::default()
        };

        let err = config.clear_network_data_dir().unwrap_err();
        assert!(
            err.to_string()
                .contains("wallet dir is inside network data dir"),
            "expected wallet protection error, got: {err}"
        );
    }

    #[test]
    fn clear_network_data_dir_leaves_sibling_networks_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let testnet_dir = tmp.path().join("testnet");
        let mainnet_dir = tmp.path().join("mainnet");
        fs::create_dir_all(&testnet_dir).unwrap();
        fs::create_dir_all(&mainnet_dir).unwrap();
        fs::write(testnet_dir.join("blocks.dat"), b"testnet data").unwrap();
        fs::write(mainnet_dir.join("blocks.dat"), b"mainnet data").unwrap();

        let config = AppConfig {
            data_dir: tmp.path().to_path_buf(),
            network: Network::Testnet,
            wallet_dir: Some(tmp.path().join("wallets")),
            ..Default::default()
        };

        config.clear_network_data_dir().unwrap();

        assert!(testnet_dir.exists(), "testnet dir should be recreated");
        assert!(
            !testnet_dir.join("blocks.dat").exists(),
            "testnet contents should be cleared"
        );
        assert!(
            fs::read_dir(&testnet_dir).unwrap().next().is_none(),
            "testnet dir should be empty"
        );
        assert!(
            mainnet_dir.exists(),
            "sibling mainnet dir must not be removed"
        );
        assert!(
            mainnet_dir.join("blocks.dat").exists(),
            "sibling mainnet files must not be removed"
        );
    }

    #[test]
    fn clear_network_data_dir_allows_default_wallet_dir() {
        let tmp = tempfile::tempdir().unwrap();
        // Default wallet_dir is <data_dir>/wallets, which is a sibling of the
        // network dir, not inside it.
        let config = AppConfig {
            data_dir: tmp.path().to_path_buf(),
            network: Network::Testnet,
            wallet_dir: None,
            ..Default::default()
        };

        config.clear_network_data_dir().unwrap();
        assert!(config.network_data_dir().exists());
    }

    #[test]
    fn invalid_mempool_strategy_is_rejected() {
        let config_path = write_temp_config(
            "invalid-mempool",
            r#"
                network = "testnet"
                data_dir = "/tmp"
                dev_mode = false
                log_level = "info"

                [testnet]
                mempool_strategy = "invalid-strategy"
            "#,
        );

        let result = AppConfig::load_with_cli(Cli::default(), &config_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid mempool_strategy"));

        let _ = std::fs::remove_dir_all(config_path.parent().unwrap());
    }

    #[test]
    fn network_config_mut_serializes_with_sensible_mempool_default() {
        let mut config = AppConfig::default();
        config.network_config_mut().peers = vec!["1.2.3.4:9999".to_string()];
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(
            !toml_str.contains("mempool_strategy = \"\""),
            "unexpected empty mempool_strategy in TOML: {toml_str}"
        );
        let restored: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.mempool_strategy(), "bloom-filter");
    }

    #[test]
    fn legacy_flat_peers_rejected_as_unknown_field() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/tmp"
            dev_mode = false
            log_level = "info"
            peers = ["1.2.3.4:19999"]
        "#;
        let result = toml::from_str::<AppConfig>(toml_str);
        assert!(result.is_err(), "top-level `peers` should be rejected");
    }

    #[test]
    fn legacy_flat_mempool_strategy_rejected_as_unknown_field() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/tmp"
            dev_mode = false
            log_level = "info"
            mempool_strategy = "fetch-all"
        "#;
        let result = toml::from_str::<AppConfig>(toml_str);
        assert!(
            result.is_err(),
            "top-level `mempool_strategy` should be rejected"
        );
    }

    #[test]
    fn per_network_config_from_toml() {
        let toml_str = r#"
            network = "testnet"
            data_dir = "/tmp"
            dev_mode = false
            log_level = "info"

            [testnet]
            peers = ["1.2.3.4:19999"]
            mempool_strategy = "fetch-all"

            [mainnet]
            peers = []
            mempool_strategy = "bloom-filter"
        "#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.peers(), &["1.2.3.4:19999"]);
        assert_eq!(config.mempool_strategy(), "fetch-all");
    }

    #[test]
    fn network_key_maps_all_variants() {
        assert_eq!(AppConfig::network_key(Network::Mainnet), "mainnet");
        assert_eq!(AppConfig::network_key(Network::Testnet), "testnet");
        assert_eq!(AppConfig::network_key(Network::Devnet), "devnet");
        assert_eq!(AppConfig::network_key(Network::Regtest), "regtest");
    }

    #[test]
    fn network_config_accessor_uses_active_network() {
        let mut config = AppConfig {
            network: Network::Mainnet,
            ..Default::default()
        };
        config.network_config_mut().mempool_strategy = "fetch-all".to_string();

        config.network = Network::Testnet;
        config.network_config_mut().mempool_strategy = "bloom-filter".to_string();

        assert_eq!(config.mempool_strategy(), "bloom-filter");

        config.network = Network::Mainnet;
        assert_eq!(config.mempool_strategy(), "fetch-all");
    }
}
