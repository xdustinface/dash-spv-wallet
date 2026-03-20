mod backend;
mod components;
mod event_bridge;
mod router;
mod screens;
mod state;

use std::fmt;

use clap::Parser;
use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::backend::types::Network;
use crate::router::Route;
use crate::state::app_state::AppState;
use crate::state::connection::ConnectionState;
use crate::state::dev_log::DevLog;
use crate::state::network::NetworkInfo;
use crate::state::wallet::WalletState;

#[derive(Parser)]
#[command(name = "dash-spv-ui", about = "Dash SPV Wallet")]
struct Cli {
    /// Enable developer mode
    #[arg(long)]
    dev: bool,

    /// Select network (skip network selection screen)
    #[arg(long, value_parser = parse_network)]
    network: Option<Network>,

    /// Select backend implementation
    #[arg(long, default_value = "native")]
    backend: BackendChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendChoice {
    Native,
    Ffi,
}

impl fmt::Display for BackendChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native => write!(f, "native"),
            Self::Ffi => write!(f, "ffi"),
        }
    }
}

impl std::str::FromStr for BackendChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "native" => Ok(Self::Native),
            "ffi" => Ok(Self::Ffi),
            other => Err(format!("unknown backend: {other} (expected `native` or `ffi`)")),
        }
    }
}

fn parse_network(s: &str) -> Result<Network, String> {
    match s.to_lowercase().as_str() {
        "mainnet" => Ok(Network::Mainnet),
        "testnet" => Ok(Network::Testnet),
        "regtest" => Ok(Network::Regtest),
        other => Err(format!(
            "unknown network: {other} (expected `mainnet`, `testnet`, or `regtest`)"
        )),
    }
}

fn main() {
    let cli = Cli::parse();

    let mut app_state = AppState::new(cli.dev);
    if let Some(network) = cli.network {
        app_state.select_network(network);
    }

    // Store CLI-derived state for the app component to pick up via context.
    APP_STATE.with(|cell| cell.set(app_state).ok());
    INITIAL_NETWORK.with(|cell| cell.set(cli.network).ok());

    dioxus::launch(app);
}

thread_local! {
    static APP_STATE: std::cell::OnceCell<AppState> = const { std::cell::OnceCell::new() };
    static INITIAL_NETWORK: std::cell::OnceCell<Option<Network>> = const { std::cell::OnceCell::new() };
}

fn app() -> Element {
    let initial_state = APP_STATE.with(|cell| cell.get().cloned().unwrap_or_else(|| AppState::new(false)));
    let initial_network = INITIAL_NETWORK.with(|cell| cell.get().copied().flatten());

    // Provide AppState as context for all components.
    let _app_state = use_context_provider(|| Signal::new(initial_state));

    // Create a MockBackend (real backends come later).
    let network = initial_network.unwrap_or(Network::Testnet);
    let _backend = use_context_provider(|| {
        Signal::new(MockBackend::builder(network).build())
    });

    // Provide reactive state signals for the event bridge.
    let _connection = use_context_provider(|| Signal::new(ConnectionState::default()));
    let _wallet = use_context_provider(|| Signal::new(WalletState::default()));
    let _network_info = use_context_provider(|| Signal::new(NetworkInfo::default()));
    let _dev_log = use_context_provider(|| Signal::new(DevLog::default()));

    rsx! {
        style { {include_str!("../assets/tailwind.css")} }
        Router::<Route> {}
    }
}
