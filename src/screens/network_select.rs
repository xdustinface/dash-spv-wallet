use dioxus::prelude::*;

use crate::backend::types::Network;
use crate::router::Route;
use crate::state::app_state::AppState;

#[component]
pub fn NetworkSelect() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let navigator = use_navigator();

    let select_network = move |network: Network| {
        let navigator = navigator;
        move |_| {
            app_state.write().select_network(network);
            navigator.push(Route::WalletChoice {});
        }
    };

    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-screen bg-surface text-foreground",

            h1 {
                class: "text-3xl font-bold mb-2",
                "Dash SPV Wallet"
            }
            p {
                class: "text-muted mb-10",
                "Select a network to get started"
            }

            div {
                class: "flex gap-6",

                NetworkCard {
                    name: "Mainnet",
                    description: "Production network",
                    color: "blue",
                    onclick: select_network(Network::Mainnet),
                }
                NetworkCard {
                    name: "Testnet",
                    description: "Testing network",
                    color: "green",
                    onclick: select_network(Network::Testnet),
                }
                NetworkCard {
                    name: "Regtest",
                    description: "Local development",
                    color: "orange",
                    onclick: select_network(Network::Regtest),
                }
            }
        }
    }
}

#[component]
fn NetworkCard(
    name: &'static str,
    description: &'static str,
    color: &'static str,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let border_class = match color {
        "blue" => "border-dash hover:bg-dash/10",
        "green" => "border-success hover:bg-success/10",
        "orange" => "border-warning hover:bg-warning/10",
        _ => "border-edge hover:bg-hover",
    };

    let dot_class = match color {
        "blue" => "bg-dash",
        "green" => "bg-success",
        "orange" => "bg-warning",
        _ => "bg-muted",
    };

    rsx! {
        div {
            class: "flex flex-col items-center p-8 bg-card border-2 rounded-xl cursor-pointer transition-colors {border_class}",
            onclick: move |evt| onclick.call(evt),

            div {
                class: "w-4 h-4 rounded-full mb-4 {dot_class}",
            }
            h2 {
                class: "text-xl font-semibold mb-1",
                "{name}"
            }
            p {
                class: "text-sm text-muted",
                "{description}"
            }
        }
    }
}
