mod backend;
mod components;
mod config;
mod event_bridge;
mod router;
mod screens;
mod state;

use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
#[cfg(feature = "ffi")]
use crate::backend::ffi::FfiBackend;
use crate::backend::mock::MockBackend;
use crate::backend::native::NativeBackend;
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

    // If a wallet mnemonic file exists, skip onboarding screens.
    let mnemonic_path = config.wallet_dir().join("wallet.mnemonic");
    if mnemonic_path.exists() {
        app_state.set_wallet_loaded();
    }

    let window_width = config.window_width;
    let window_height = config.window_height;

    // Store startup state for the app component to pick up via context.
    APP_STATE.with(|cell| cell.set(app_state).ok());
    CONFIG.with(|cell| cell.set(config).ok());

    let mut window_builder = dioxus::desktop::WindowBuilder::new()
        .with_title("Dash SPV Wallet")
        .with_inner_size(dioxus::desktop::LogicalSize::new(
            window_width,
            window_height,
        ));

    #[cfg(target_os = "macos")]
    {
        use dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS;
        window_builder = window_builder
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true);
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus::desktop::Config::new().with_window(window_builder))
        .launch(app);
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
    let mock_mode = config.mock_mode;

    // Provide AppState and AppConfig as context for all components.
    let _app_state = use_context_provider(|| Signal::new(initial_state));

    // Select backend based on configuration.
    let dev_mode = config.dev_mode;
    let backend_name = config.backend.clone();
    let _backend = use_context_provider(|| {
        let backend = if mock_mode {
            Backend::Mock(MockBackend::builder(network).build())
        } else if dev_mode && backend_name == "ffi" {
            #[cfg(feature = "ffi")]
            {
                Backend::Ffi(FfiBackend::new(config.clone()))
            }
            #[cfg(not(feature = "ffi"))]
            {
                eprintln!("FFI backend requested but `ffi` feature not enabled, falling back to native");
                Backend::Native(NativeBackend::new(config.clone()))
            }
        } else {
            Backend::Native(NativeBackend::new(config.clone()))
        };
        Signal::new(backend)
    });

    let _config = use_context_provider(|| Signal::new(config));

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
