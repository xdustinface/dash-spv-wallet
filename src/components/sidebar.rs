use dioxus::prelude::*;

use crate::router::Route;

#[component]
pub fn Sidebar() -> Element {
    rsx! {
        nav {
            class: "flex flex-col w-56 min-h-screen bg-gray-800 text-gray-300",

            div {
                class: "p-4 text-xl font-bold text-blue-500 border-b border-gray-700",
                "Dash SPV"
            }

            div {
                class: "flex flex-col flex-1 p-2 space-y-1",

                NavLink { to: Route::Dashboard {}, label: "Dashboard" }
                NavLink { to: Route::Send {}, label: "Send" }
                NavLink { to: Route::Receive {}, label: "Receive" }
            }
        }
    }
}

#[component]
fn NavLink(to: Route, label: &'static str) -> Element {
    rsx! {
        Link {
            to,
            class: "block px-4 py-2 rounded hover:bg-gray-700 hover:text-white transition-colors",
            "{label}"
        }
    }
}
