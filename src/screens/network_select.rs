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
            class: "flex flex-col items-center justify-center min-h-screen bg-gray-900 text-white",

            h1 {
                class: "text-3xl font-bold mb-2",
                "Dash SPV Wallet"
            }
            p {
                class: "text-gray-400 mb-10",
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
        "blue" => "border-blue-500 hover:bg-blue-500/10",
        "green" => "border-green-500 hover:bg-green-500/10",
        "orange" => "border-orange-500 hover:bg-orange-500/10",
        _ => "border-gray-500 hover:bg-gray-500/10",
    };

    let dot_class = match color {
        "blue" => "bg-blue-500",
        "green" => "bg-green-500",
        "orange" => "bg-orange-500",
        _ => "bg-gray-500",
    };

    rsx! {
        div {
            class: "flex flex-col items-center p-8 bg-gray-800 border-2 rounded-xl cursor-pointer transition-colors {border_class}",
            onclick: move |evt| onclick.call(evt),

            div {
                class: "w-4 h-4 rounded-full mb-4 {dot_class}",
            }
            h2 {
                class: "text-xl font-semibold mb-1",
                "{name}"
            }
            p {
                class: "text-sm text-gray-400",
                "{description}"
            }
        }
    }
}
