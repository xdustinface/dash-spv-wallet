mod backend;
mod components;
mod config;
mod event_bridge;
mod router;
mod screens;
mod state;

use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::config::AppConfig;
use crate::router::Route;
use crate::state::app_state::AppState;
use crate::state::connection::ConnectionState;
use crate::state::dev_log::DevLog;
use crate::state::network::NetworkInfo;
use crate::state::wallet::WalletState;

fn main() {
    let config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("Config error: {e}");
        AppConfig::default()
    });

    if let Err(e) = config.ensure_dirs() {
        eprintln!("Failed to create directories: {e}");
    }

    let mut app_state = AppState::new(config.dev_mode);
    app_state.select_network(config.network);

    // Store startup state for the app component to pick up via context.
    APP_STATE.with(|cell| cell.set(app_state).ok());
    CONFIG.with(|cell| cell.set(config).ok());

    dioxus::launch(app);
}

thread_local! {
    static APP_STATE: std::cell::OnceCell<AppState> = const { std::cell::OnceCell::new() };
    static CONFIG: std::cell::OnceCell<AppConfig> = const { std::cell::OnceCell::new() };
}

fn app() -> Element {
    let initial_state =
        APP_STATE.with(|cell| cell.get().cloned().unwrap_or_else(|| AppState::new(false)));
    let config = CONFIG.with(|cell| cell.get().cloned().unwrap_or_default());

    let network = config.network;

    // Provide AppState and AppConfig as context for all components.
    let _app_state = use_context_provider(|| Signal::new(initial_state));
    let _config = use_context_provider(|| Signal::new(config));

    // Create a MockBackend (real backends come later).
    let _backend =
        use_context_provider(|| Signal::new(MockBackend::builder(network).build()));

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
