use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::backend::r#trait::SpvBackend;
use crate::state::connection::ConnectionState;
use crate::state::network::NetworkInfo;
use crate::state::view_models::format_peer_count;

#[component]
pub fn StatusBar() -> Element {
    let connection = use_context::<Signal<ConnectionState>>();
    let network_info = use_context::<Signal<NetworkInfo>>();
    let backend = use_context::<Signal<MockBackend>>();

    let conn = connection.read();
    let info = network_info.read();

    let (dot_color, status_text, animate) = match &*conn {
        ConnectionState::Disconnected => ("bg-muted", "Disconnected".to_string(), false),
        ConnectionState::Connecting => ("bg-warning", "Connecting...".to_string(), true),
        ConnectionState::Syncing(progress) => (
            "bg-dash",
            format!("Syncing {:.0}%", progress.percentage),
            true,
        ),
        ConnectionState::Synced => ("bg-success", "Synced".to_string(), false),
        ConnectionState::Paused => ("bg-warning", "Paused".to_string(), false),
        ConnectionState::Error(msg) => ("bg-error", msg.clone(), false),
    };

    let can_pause = matches!(&*conn, ConnectionState::Syncing(_) | ConnectionState::Synced | ConnectionState::Connecting);
    let can_resume = matches!(&*conn, ConnectionState::Paused);

    let peer_text = format_peer_count(info.connected_peers);

    let chain_tip = if info.chain_tip > 0 {
        format!("Height {}", info.chain_tip)
    } else {
        String::new()
    };

    let dot_class = if animate {
        format!("inline-block w-2 h-2 rounded-full {dot_color} mr-2 animate-pulse")
    } else {
        format!("inline-block w-2 h-2 rounded-full {dot_color} mr-2")
    };

    rsx! {
        div {
            class: "flex items-center px-4 py-2 bg-surface-alt border-t border-edge text-sm text-muted",

            // Status indicator
            span { class: "{dot_class}" }
            span { class: "mr-4", "{status_text}" }

            // Peer count
            if info.connected_peers > 0 {
                span {
                    class: "mr-4 text-disabled",
                    "{peer_text}"
                }
            }

            // Chain tip
            if !chain_tip.is_empty() {
                span {
                    class: "mr-4 text-disabled",
                    "{chain_tip}"
                }
            }

            // Spacer
            div { class: "flex-1" }

            // Pause/Resume button
            if can_pause {
                button {
                    class: "px-3 py-1 text-xs rounded bg-hover hover:bg-edge text-muted transition-colors",
                    onclick: move |_| async move {
                        let _ = backend.read().pause().await;
                        use_context::<Signal<ConnectionState>>().write().pause();
                    },
                    "Pause"
                }
            }
            if can_resume {
                button {
                    class: "px-3 py-1 text-xs rounded bg-hover hover:bg-edge text-muted transition-colors",
                    onclick: move |_| async move {
                        let _ = backend.read().resume().await;
                        use_context::<Signal<ConnectionState>>().write().resume();
                    },
                    "Resume"
                }
            }
        }
    }
}
