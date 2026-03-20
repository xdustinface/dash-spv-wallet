use dioxus::prelude::*;

use crate::router::Route;

#[component]
pub fn Sidebar() -> Element {
    let route: Route = use_route();

    rsx! {
        nav {
            class: "flex flex-col w-56 h-full overflow-y-auto bg-surface text-muted border-r border-edge",

            div {
                class: "px-4 py-3 text-lg font-bold text-foreground tracking-wide bg-dash-dark",
                span { class: "text-dash", "Dash" }
                span { class: "ml-1 text-muted font-normal text-sm", "SPV" }
            }

            div {
                class: "flex flex-col flex-1 p-2 space-y-1",

                NavLink { to: Route::Dashboard {}, label: "Dashboard", active: matches!(route, Route::Dashboard {}) }
                NavLink { to: Route::Transactions {}, label: "Transactions", active: matches!(route, Route::Transactions {}) }
                NavLink { to: Route::Send {}, label: "Send", active: matches!(route, Route::Send {}) }
                NavLink { to: Route::Receive {}, label: "Receive", active: matches!(route, Route::Receive {}) }
            }

            div {
                class: "p-2 mt-auto",
                NavLink { to: Route::Settings {}, label: "Settings", active: matches!(route, Route::Settings {}) }
            }
        }
    }
}

#[component]
fn NavLink(to: Route, label: &'static str, active: bool) -> Element {
    let class = if active {
        "block px-4 py-2 rounded-r bg-dash/10 text-dash border-l-2 border-dash font-medium transition-colors"
    } else {
        "block px-4 py-2 rounded text-muted hover:bg-hover hover:text-foreground transition-colors"
    };

    rsx! {
        Link {
            to,
            class,
            "{label}"
        }
    }
}
