pub mod dev_panel;
pub mod layout;
pub mod sidebar;
pub mod status_bar;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use dioxus::prelude::*;

    use crate::backend::dispatch::Backend;
    use crate::backend::mock::MockBackend;
    use crate::backend::types::Network;
    use crate::config::AppConfig;
    use crate::state::app_state::AppState;
    use crate::state::connection::{ConnectionState, SyncProgressView};
    use crate::state::dev_log::DevLog;
    use crate::state::network::NetworkInfo;
    use crate::state::wallet::WalletState;

    fn test_config() -> AppConfig {
        AppConfig {
            network: Network::Testnet,
            data_dir: PathBuf::from("/tmp/dash-spv-test"),
            ..Default::default()
        }
    }

    fn test_backend() -> Backend {
        Backend::Mock(MockBackend::builder(Network::Testnet).build())
    }

    fn test_app_state(dev_mode: bool) -> AppState {
        let mut state = AppState::new(dev_mode);
        state.select_network(Network::Testnet);
        state
    }

    fn use_test_contexts() {
        use_context_provider(|| Signal::new(test_backend()));
        use_context_provider(|| Signal::new(test_config()));
        use_context_provider(|| Signal::new(test_app_state(false)));
        use_context_provider(|| Signal::new(WalletState::default()));
        use_context_provider(|| Signal::new(NetworkInfo::default()));
        use_context_provider(|| Signal::new(ConnectionState::default()));
        use_context_provider(|| Signal::new(DevLog::default()));
    }

    fn use_dev_mode_contexts() {
        use_context_provider(|| Signal::new(test_backend()));
        use_context_provider(|| Signal::new(test_config()));
        use_context_provider(|| Signal::new(test_app_state(true)));
        use_context_provider(|| Signal::new(WalletState::default()));
        use_context_provider(|| Signal::new(NetworkInfo::default()));
        use_context_provider(|| Signal::new(ConnectionState::default()));
        use_context_provider(|| Signal::new(DevLog::default()));
    }

    fn render_component(component: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(component);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    // -- StatusBar --

    #[component]
    fn status_bar_disconnected_wrapper() -> Element {
        use_test_contexts();
        rsx! { super::status_bar::StatusBar {} }
    }

    #[test]
    fn status_bar_renders_disconnected() {
        let html = render_component(status_bar_disconnected_wrapper);
        assert!(
            html.contains("Disconnected"),
            "expected disconnected status"
        );
        assert!(
            html.to_lowercase().contains("testnet"),
            "expected network name"
        );
    }

    #[component]
    fn status_bar_syncing_wrapper() -> Element {
        use_context_provider(|| Signal::new(test_backend()));
        use_context_provider(|| Signal::new(test_config()));
        let mut app_state = AppState::new(false);
        app_state.select_network(Network::Testnet);
        use_context_provider(|| Signal::new(app_state));
        use_context_provider(|| Signal::new(WalletState::default()));
        use_context_provider(|| {
            Signal::new(NetworkInfo {
                connected_peers: 3,
                best_height: 50000,
                chain_tip: 45000,
            })
        });
        use_context_provider(|| {
            Signal::new(ConnectionState::Syncing(SyncProgressView {
                percentage: 75.0,
                stage_label: "Syncing headers...".into(),
            }))
        });
        use_context_provider(|| Signal::new(DevLog::default()));

        rsx! { super::status_bar::StatusBar {} }
    }

    #[test]
    fn status_bar_renders_syncing_with_peers() {
        let html = render_component(status_bar_syncing_wrapper);
        assert!(html.contains("75%"), "expected sync percentage");
        assert!(html.contains("3 peers"), "expected peer count");
        assert!(html.contains("Height 45000"), "expected chain tip height");
    }

    #[component]
    fn status_bar_synced_wrapper() -> Element {
        use_context_provider(|| Signal::new(test_backend()));
        use_context_provider(|| Signal::new(test_config()));
        let mut app_state = AppState::new(false);
        app_state.select_network(Network::Testnet);
        use_context_provider(|| Signal::new(app_state));
        use_context_provider(|| Signal::new(WalletState::default()));
        use_context_provider(|| {
            Signal::new(NetworkInfo {
                connected_peers: 1,
                best_height: 50000,
                chain_tip: 50000,
            })
        });
        use_context_provider(|| Signal::new(ConnectionState::Synced));
        use_context_provider(|| Signal::new(DevLog::default()));

        rsx! { super::status_bar::StatusBar {} }
    }

    #[test]
    fn status_bar_renders_synced() {
        let html = render_component(status_bar_synced_wrapper);
        assert!(html.contains("Synced"), "expected synced status");
        assert!(html.contains("1 peer"), "expected singular peer count");
    }

    // -- DevPanel --

    #[component]
    fn dev_panel_hidden_wrapper() -> Element {
        use_test_contexts();
        rsx! { super::dev_panel::DevPanel {} }
    }

    #[test]
    fn dev_panel_hidden_when_not_dev_mode() {
        let html = render_component(dev_panel_hidden_wrapper);
        assert!(
            !html.contains("Dev Log"),
            "dev panel should be hidden when dev_mode is false"
        );
    }

    #[component]
    fn dev_panel_visible_wrapper() -> Element {
        use_dev_mode_contexts();
        rsx! { super::dev_panel::DevPanel {} }
    }

    #[test]
    fn dev_panel_renders_when_dev_mode() {
        let html = render_component(dev_panel_visible_wrapper);
        assert!(html.contains("Dev Log"), "expected Dev Log toggle");
        assert!(html.contains("Expand"), "expected Expand toggle text");
    }

    #[test]
    fn dev_panel_shows_empty_entry_count() {
        let html = render_component(dev_panel_visible_wrapper);
        assert!(html.contains("[0]"), "expected zero entry count");
    }

    #[component]
    fn dev_panel_with_entries_wrapper() -> Element {
        use_context_provider(|| Signal::new(test_backend()));
        use_context_provider(|| Signal::new(test_config()));
        use_context_provider(|| Signal::new(test_app_state(true)));
        use_context_provider(|| Signal::new(WalletState::default()));
        use_context_provider(|| Signal::new(NetworkInfo::default()));
        use_context_provider(|| Signal::new(ConnectionState::default()));

        let mut log = DevLog::default();
        use crate::backend::events::SpvEvent;
        log.push(&SpvEvent::PeerConnected("10.0.0.1".into()));
        log.push(&SpvEvent::HeadersSynced { tip_height: 500 });
        log.push(&SpvEvent::Error("timeout".into()));
        use_context_provider(|| Signal::new(log));

        rsx! { super::dev_panel::DevPanel {} }
    }

    #[test]
    fn dev_panel_shows_nonzero_entry_count() {
        let html = render_component(dev_panel_with_entries_wrapper);
        assert!(html.contains("[3]"), "expected entry count of 3");
    }
}
