pub mod dashboard;
pub mod network_select;
pub mod receive;
pub mod send;
pub mod settings;
pub mod transactions;
pub mod wallet_choice;
pub mod wallet_create;
pub mod wallet_import;

#[cfg(test)]
mod tests {

    use std::rc::Rc;

    use dioxus::prelude::*;
    use dioxus_history::{MemoryHistory, provide_history_context};

    use crate::backend::dispatch::Backend;
    use crate::backend::mock::MockBackend;
    use crate::backend::types::Network;
    use crate::config::AppConfig;
    use crate::router::Route;
    use crate::state::app_state::AppState;
    use crate::state::connection::ConnectionState;
    use crate::state::dev_log::DevLog;
    use crate::state::network::NetworkInfo;
    use crate::state::wallet::WalletState;

    fn test_config() -> AppConfig {
        AppConfig {
            network: Network::Testnet,
            data_dir: std::env::temp_dir().join(format!("dash-spv-test-{}", std::process::id())),
            ..Default::default()
        }
    }

    fn test_backend() -> Backend {
        Backend::Mock(MockBackend::builder(Network::Testnet).build())
    }

    /// Provides all context signals that screen components expect.
    fn use_test_contexts() {
        use_context_provider(|| Signal::new(test_backend()));
        use_context_provider(|| Signal::new(test_config()));
        use_context_provider(|| Signal::new(AppState::new(false)));
        use_context_provider(|| Signal::new(WalletState::default()));
        use_context_provider(|| Signal::new(NetworkInfo::default()));
        use_context_provider(|| Signal::new(ConnectionState::default()));
        use_context_provider(|| Signal::new(DevLog::default()));
    }

    /// Renders a component via SSR with all required context signals.
    fn render_screen(screen: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(screen);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    // -- Wrapper components for each screen --
    // Each provides context and delegates to the actual screen component.

    #[component]
    fn send_wrapper() -> Element {
        use_test_contexts();
        super::send::Send()
    }

    #[component]
    fn receive_wrapper() -> Element {
        use_test_contexts();
        super::receive::Receive()
    }

    #[component]
    fn transactions_wrapper() -> Element {
        use_test_contexts();
        super::transactions::Transactions()
    }

    #[component]
    fn settings_wrapper() -> Element {
        use_test_contexts();
        super::settings::Settings()
    }

    #[component]
    fn dashboard_wrapper() -> Element {
        use_test_contexts();
        super::dashboard::Dashboard()
    }

    /// Creates a routed wrapper that renders the screen at the given URL via
    /// a `Router::<Route>` with a `MemoryHistory` starting at that path.
    macro_rules! routed_wrapper {
        ($name:ident, $url:expr) => {
            #[component]
            fn $name() -> Element {
                use_test_contexts();
                provide_history_context(Rc::new(MemoryHistory::with_initial_path($url)));
                rsx! { Router::<Route> {} }
            }
        };
    }

    routed_wrapper!(network_select_routed, "/");
    routed_wrapper!(wallet_choice_routed, "/wallet");
    routed_wrapper!(wallet_create_routed, "/wallet/create");
    routed_wrapper!(wallet_import_routed, "/wallet/import");

    #[test]
    fn send_screen_renders() {
        let html = render_screen(send_wrapper);
        assert!(html.contains("Send"), "expected 'Send' heading");
        assert!(
            html.contains("Destination Address"),
            "expected address label"
        );
        assert!(html.contains("Amount"), "expected amount label");
        assert!(html.contains("Fee Rate"), "expected fee rate label");
        assert!(html.contains("Economy"), "expected economy fee option");
        assert!(html.contains("Normal"), "expected normal fee option");
        assert!(html.contains("Priority"), "expected priority fee option");
        assert!(html.contains("Review"), "expected review button");
    }

    #[test]
    fn receive_screen_renders() {
        let html = render_screen(receive_wrapper);
        assert!(html.contains("Receive"), "expected 'Receive' heading");
        assert!(
            html.contains("No address generated yet"),
            "expected empty address placeholder"
        );
        assert!(
            html.contains("Generate New Address"),
            "expected generate button"
        );
    }

    #[test]
    fn transactions_screen_renders() {
        let html = render_screen(transactions_wrapper);
        assert!(
            html.contains("Transactions"),
            "expected 'Transactions' heading"
        );
        assert!(
            html.contains("No transactions yet"),
            "expected empty state message"
        );
        assert!(html.contains("All"), "expected 'All' filter tab");
        assert!(html.contains("Received"), "expected 'Received' filter tab");
        assert!(html.contains("Sent"), "expected 'Sent' filter tab");
        assert!(
            html.contains("Search by txid"),
            "expected search placeholder"
        );
    }

    #[test]
    fn settings_screen_renders() {
        let html = render_screen(settings_wrapper);
        assert!(html.contains("Settings"), "expected 'Settings' heading");
        assert!(html.contains("Network"), "expected network selector");
        assert!(html.contains("Mainnet"), "expected mainnet option");
        assert!(html.contains("Testnet"), "expected testnet option");
        assert!(html.contains("Regtest"), "expected regtest option");
        assert!(html.contains("Data Directory"), "expected data dir field");
        assert!(html.contains("Log Level"), "expected log level selector");
        assert!(html.contains("Save"), "expected save button");
    }

    #[test]
    fn dashboard_screen_renders() {
        let html = render_screen(dashboard_wrapper);
        assert!(
            html.contains("Available Balance"),
            "expected balance heading"
        );
        assert!(
            html.contains("No transactions yet"),
            "expected empty tx message"
        );
    }

    #[test]
    fn network_select_screen_renders() {
        let html = render_screen(network_select_routed);
        assert!(html.contains("Dash SPV Wallet"), "expected app title");
        assert!(html.contains("Select a network"), "expected subtitle");
        assert!(html.contains("Mainnet"), "expected mainnet card");
        assert!(html.contains("Testnet"), "expected testnet card");
        assert!(html.contains("Regtest"), "expected regtest card");
    }

    #[test]
    fn wallet_choice_screen_renders() {
        let html = render_screen(wallet_choice_routed);
        assert!(html.contains("Wallet Setup"), "expected heading");
        assert!(html.contains("Create New Wallet"), "expected create option");
        assert!(
            html.contains("Import Existing Wallet"),
            "expected import option"
        );
    }

    #[test]
    fn wallet_create_screen_renders() {
        let html = render_screen(wallet_create_routed);
        assert!(html.contains("Create New Wallet"), "expected heading");
        assert!(html.contains("Generate"), "expected generate button text");
    }

    #[test]
    fn wallet_import_screen_renders() {
        let html = render_screen(wallet_import_routed);
        assert!(html.contains("Import Wallet"), "expected heading");
        assert!(
            html.contains("recovery phrase"),
            "expected recovery phrase text"
        );
        assert!(
            html.contains("0 of 12 words"),
            "expected word count display"
        );
    }
}
