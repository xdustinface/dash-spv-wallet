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
            class: "flex min-h-screen bg-gray-900 text-white",

            Sidebar {}

            div {
                class: "flex flex-col flex-1",

                header {
                    class: "flex items-center justify-end px-4 py-2 bg-gray-850 border-b border-gray-700",
                    NetworkBadge { label: network_label }
                }

                main {
                    class: "flex-1 p-6",
                    Outlet::<crate::router::Route> {}
                }

                DevPanel {}
                StatusBar {}
            }
        }
    }
}

#[component]
fn NetworkBadge(label: String) -> Element {
    rsx! {
        span {
            class: "px-3 py-1 text-xs font-medium rounded-full bg-gray-700 text-gray-300",
            "{label}"
        }
    }
}
