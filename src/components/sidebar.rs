use dioxus::prelude::*;

use crate::router::Route;

#[component]
pub fn Sidebar() -> Element {
    let route: Route = use_route();

    rsx! {
        nav { class: "flex flex-col w-56 h-full overflow-y-auto bg-surface text-muted border-r border-edge",

            div {
                class: "flex flex-col items-center pt-10 pb-4 px-6 bg-dash-dark cursor-default select-none",
                onmousedown: move |_| {
                    dioxus::desktop::window().drag();
                },

                div {
                    class: "w-32 mb-1",
                    dangerous_inner_html: include_str!("../../assets/dash-logo.svg"),
                }

                span { class: "text-xs font-semibold text-foreground tracking-wide uppercase",
                    "SPV Wallet"
                }
            }

            div { class: "flex flex-col flex-1 p-2 space-y-1",

                NavLink {
                    to: Route::Dashboard {},
                    label: "Dashboard",
                    active: matches!(route, Route::Dashboard {}),
                }
                NavLink {
                    to: Route::Transactions {},
                    label: "Transactions",
                    active: matches!(route, Route::Transactions {}),
                }
                NavLink {
                    to: Route::Send {},
                    label: "Send",
                    active: matches!(route, Route::Send {}),
                }
                NavLink {
                    to: Route::Receive {},
                    label: "Receive",
                    active: matches!(route, Route::Receive {}),
                }
            }

            div { class: "p-2 mt-auto",
                NavLink {
                    to: Route::Settings {},
                    label: "Settings",
                    active: matches!(route, Route::Settings {}),
                }
            }
        }
    }
}

#[component]
fn NavLink(to: Route, label: &'static str, active: bool) -> Element {
    let class = if active {
        "block px-4 py-2 rounded-r bg-dash/10 text-dash border-l-2 border-dash font-medium transition-colors"
    } else {
        "block px-4 py-2 rounded text-muted hover:bg-hover hover:text-foreground transition-colors border-l-2 border-transparent"
    };

    rsx! {
        Link { to, class, "{label}" }
    }
}
