use dioxus::prelude::*;

use crate::backend::types::Network;
use crate::config::AppConfig;
use crate::router::Route;
use crate::state::app_state::AppState;

#[component]
pub fn NetworkSelect() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let mut config = use_context::<Signal<AppConfig>>();
    let navigator = use_navigator();

    // If a wallet is already loaded (mnemonic file found at startup), skip
    // onboarding and go straight to the dashboard.
    use_effect(move || {
        if app_state.read().wallet_loaded {
            navigator.push(Route::Dashboard {});
        }
    });

    let select_network = move |network: Network| {
        let navigator = navigator;
        move |_| {
            app_state.write().select_network(network);
            let mut cfg = config.write();
            cfg.network = network;
            let _ = cfg.save(&AppConfig::default_config_path());
            navigator.push(Route::WalletChoice {});
        }
    };

    rsx! {
        div { class: "flex flex-col items-center justify-center min-h-screen bg-surface text-foreground",

            h1 { class: "text-3xl font-bold mb-2", "Dash SPV Wallet" }
            p { class: "text-muted mb-10", "Select a network to get started" }

            div { class: "flex gap-6",

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

fn border_class_for_color(color: &str) -> &'static str {
    match color {
        "blue" => "border-dash hover:bg-dash/10",
        "green" => "border-success hover:bg-success/10",
        "orange" => "border-warning hover:bg-warning/10",
        _ => "border-edge hover:bg-hover",
    }
}

fn dot_class_for_color(color: &str) -> &'static str {
    match color {
        "blue" => "bg-dash",
        "green" => "bg-success",
        "orange" => "bg-warning",
        _ => "bg-muted",
    }
}

#[component]
fn NetworkCard(
    name: &'static str,
    description: &'static str,
    color: &'static str,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let border_class = border_class_for_color(color);
    let dot_class = dot_class_for_color(color);

    rsx! {
        div {
            class: "flex flex-col items-center p-8 bg-card border-2 rounded-xl cursor-pointer transition-colors {border_class}",
            onclick: move |evt| onclick.call(evt),

            div { class: "w-4 h-4 rounded-full mb-4 {dot_class}" }
            h2 { class: "text-xl font-semibold mb-1", "{name}" }
            p { class: "text-sm text-muted", "{description}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_class_known_colors() {
        assert_eq!(
            border_class_for_color("blue"),
            "border-dash hover:bg-dash/10"
        );
        assert_eq!(
            border_class_for_color("green"),
            "border-success hover:bg-success/10"
        );
        assert_eq!(
            border_class_for_color("orange"),
            "border-warning hover:bg-warning/10"
        );
    }

    #[test]
    fn border_class_unknown_color() {
        assert_eq!(border_class_for_color("red"), "border-edge hover:bg-hover");
        assert_eq!(border_class_for_color(""), "border-edge hover:bg-hover");
    }

    #[test]
    fn dot_class_known_colors() {
        assert_eq!(dot_class_for_color("blue"), "bg-dash");
        assert_eq!(dot_class_for_color("green"), "bg-success");
        assert_eq!(dot_class_for_color("orange"), "bg-warning");
    }

    #[test]
    fn dot_class_unknown_color() {
        assert_eq!(dot_class_for_color("red"), "bg-muted");
    }
}
