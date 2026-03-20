use dioxus::prelude::*;

use crate::components::dev_panel::DevPanel;
use crate::components::sidebar::Sidebar;
use crate::components::status_bar::StatusBar;
use crate::state::app_state::AppState;

#[component]
pub fn AppLayout() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let network_label = app_state
        .read()
        .network
        .map(|n| n.to_string())
        .unwrap_or_else(|| "No network".to_string());

    rsx! {
        div {
            class: "flex flex-col h-screen bg-surface text-foreground",

            // Top section: sidebar + content
            div {
                class: "flex flex-1 min-h-0",

                Sidebar {}

                div {
                    class: "flex flex-col flex-1 min-h-0",

                    header {
                        class: "flex items-center justify-end px-4 py-2 bg-surface-alt border-b border-edge",
                        NetworkBadge { label: network_label }
                    }

                    main {
                        class: "flex-1 overflow-y-auto p-6",
                        Outlet::<crate::router::Route> {}
                    }

                    DevPanel {}
                }
            }

            // Full-width status bar at bottom
            StatusBar {}
        }
    }
}

#[component]
fn NetworkBadge(label: String) -> Element {
    rsx! {
        span {
            class: "px-3 py-1 text-xs font-medium rounded-full bg-hover text-muted",
            "{label}"
        }
    }
}
