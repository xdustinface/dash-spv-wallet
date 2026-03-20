use dioxus::prelude::*;

use crate::router::Route;

#[component]
pub fn Sidebar() -> Element {
    rsx! {
        nav {
            class: "flex flex-col w-56 min-h-screen bg-surface text-muted",

            div {
                class: "p-4 text-xl font-bold text-foreground bg-dash-dark",
                "Dash SPV"
            }

            div {
                class: "flex flex-col flex-1 p-2 space-y-1",

                NavLink { to: Route::Dashboard {}, label: "Dashboard" }
                NavLink { to: Route::Send {}, label: "Send" }
                NavLink { to: Route::Receive {}, label: "Receive" }
            }

            div {
                class: "p-2 mt-auto",
                NavLink { to: Route::Settings {}, label: "Settings" }
            }
        }
    }
}

#[component]
fn NavLink(to: Route, label: &'static str) -> Element {
    rsx! {
        Link {
            to,
            class: "block px-4 py-2 rounded hover:bg-hover hover:text-foreground transition-colors",
            "{label}"
        }
    }
}
