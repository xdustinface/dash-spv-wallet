use std::str::FromStr;

use dashcore::address::NetworkUnchecked;
use dashcore::{Address as DashAddress, Network};
use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::config::AppConfig;
use crate::state::view_models::{format_balance, parse_dash_amount};
use crate::state::wallet::WalletState;

#[derive(Debug, Clone, PartialEq)]
enum SendStep {
    Form,
    Review,
    Sending,
    Success(String),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FeeRate {
    Economy,
    Normal,
    Priority,
}

impl FeeRate {
    fn label(self) -> &'static str {
        match self {
            FeeRate::Economy => "Economy",
            FeeRate::Normal => "Normal",
            FeeRate::Priority => "Priority",
        }
    }

    fn description(self) -> &'static str {
        match self {
            FeeRate::Economy => "~1 sat/byte",
            FeeRate::Normal => "~10 sat/byte",
            FeeRate::Priority => "~100 sat/byte",
        }
    }

    fn sat_per_kb(self) -> u32 {
        match self {
            FeeRate::Economy => 1_000,
            FeeRate::Normal => 10_000,
            FeeRate::Priority => 100_000,
        }
    }

    const ALL: [FeeRate; 3] = [FeeRate::Economy, FeeRate::Normal, FeeRate::Priority];
}

/// Validate a Dash address string for a given network.
fn validate_send_address(address: &str, network: Network) -> Result<(), String> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return Err("Address is required".to_string());
    }
    let unchecked = DashAddress::<NetworkUnchecked>::from_str(trimmed)
        .map_err(|_| "Invalid address format".to_string())?;
    if !unchecked.is_valid_for_network(network) {
        return Err("Address is for a different network".to_string());
    }
    Ok(())
}

/// Validate a send amount against the spendable balance.
fn validate_send_amount(amount_str: &str, spendable: u64) -> Result<u64, String> {
    match parse_dash_amount(amount_str) {
        None => Err("Invalid amount".to_string()),
        Some(0) => Err("Amount must be greater than 0".to_string()),
        Some(sats) if sats > spendable => Err("Exceeds spendable balance".to_string()),
        Some(sats) => Ok(sats),
    }
}

/// Format a satoshi amount as a user-friendly DASH string for the "Max" button.
fn format_max_amount(sats: u64) -> String {
    let whole = sats / 100_000_000;
    let frac = sats % 100_000_000;
    if frac == 0 {
        format!("{whole}")
    } else {
        let frac_str = format!("{frac:08}").trim_end_matches('0').to_string();
        format!("{whole}.{frac_str}")
    }
}

#[component]
pub fn Send() -> Element {
    let backend = use_context::<Signal<Backend>>();
    let wallet = use_context::<Signal<WalletState>>();
    let config = use_context::<Signal<AppConfig>>();

    let mut address = use_signal(String::new);
    let mut amount_str = use_signal(String::new);
    let mut step = use_signal(|| SendStep::Form);
    let mut address_error = use_signal(|| None::<String>);
    let mut amount_error = use_signal(|| None::<String>);
    let mut fee_rate = use_signal(|| FeeRate::Normal);

    let spendable = wallet.read().balance.spendable();
    let unit = config.read().network.currency_unit();

    let mut validate_form = move || -> bool {
        let mut valid = true;

        let network = config.read().network;
        match validate_send_address(&address.read(), network) {
            Ok(()) => address_error.set(None),
            Err(e) => {
                address_error.set(Some(e));
                valid = false;
            }
        }

        match validate_send_amount(&amount_str.read(), spendable) {
            Ok(_) => amount_error.set(None),
            Err(e) => {
                amount_error.set(Some(e));
                valid = false;
            }
        }

        valid
    };

    // Compute fee estimate reactively
    let fee_estimate = use_memo(move || {
        let addr = address.read();
        let amt_str = amount_str.read();
        let rate = fee_rate.read();

        let amount_sats = match parse_dash_amount(&amt_str) {
            Some(sats) if sats > 0 => sats,
            _ => return None,
        };

        if addr.trim().is_empty() {
            return None;
        }

        backend
            .read()
            .estimate_fee(&addr, amount_sats, rate.sat_per_kb())
            .ok()
    });

    match step() {
        SendStep::Form => {
            rsx! {
                div { class: "text-foreground p-6",

                    h1 { class: "text-2xl font-bold mb-6", "Send" }

                    div { class: "bg-card rounded-lg p-6 max-w-xl",

                        // Address input
                        div { class: "mb-4",
                            label { class: "block text-muted text-sm mb-1", "Destination Address" }
                            input {
                                class: "w-full bg-surface-alt text-foreground rounded-lg p-3 outline-none focus:ring-2 focus:ring-dash",
                                r#type: "text",
                                placeholder: "Dash address",
                                value: "{address}",
                                oninput: move |e| {
                                    address.set(e.value());
                                    address_error.set(None);
                                },
                            }
                            if let Some(err) = address_error() {
                                p { class: "text-error text-sm mt-1", "{err}" }
                            }
                        }

                        // Amount input
                        div { class: "mb-4",
                            label { class: "block text-muted text-sm mb-1", "Amount ({unit})" }
                            div { class: "relative",
                                input {
                                    class: "w-full bg-surface-alt text-foreground rounded-lg p-3 pr-16 outline-none focus:ring-2 focus:ring-dash",
                                    r#type: "text",
                                    placeholder: "0.0",
                                    value: "{amount_str}",
                                    oninput: move |e| {
                                        amount_str.set(e.value());
                                        amount_error.set(None);
                                    },
                                }
                                button {
                                    class: "absolute right-2 top-1/2 -translate-y-1/2 bg-hover hover:bg-edge text-muted text-xs px-3 py-1 rounded transition-colors",
                                    onclick: move |_| {
                                        amount_str.set(format_max_amount(spendable));
                                        amount_error.set(None);
                                    },
                                    "Max"
                                }
                            }
                            p { class: "text-disabled text-xs mt-1",
                                "Available: {format_balance(spendable, unit)}"
                            }
                            if let Some(err) = amount_error() {
                                p { class: "text-error text-sm mt-1", "{err}" }
                            }
                        }

                        // Fee rate selector
                        div { class: "mb-4",
                            label { class: "block text-muted text-sm mb-1", "Fee Rate" }
                            div { class: "flex border border-edge rounded-lg overflow-hidden",
                                for (i , rate) in FeeRate::ALL.iter().enumerate() {
                                    {
                                        let rate = *rate;
                                        let active = fee_rate() == rate;
                                        let border = if i > 0 { "border-l border-edge" } else { "" };
                                        let bg = if active {
                                            "bg-dash text-foreground"
                                        } else {
                                            "bg-surface-alt text-muted hover:bg-hover"
                                        };
                                        rsx! {
                                            button {
                                                class: "flex-1 py-2 text-center transition-colors {bg} {border}",
                                                onclick: move |_| fee_rate.set(rate),
                                                span { class: "text-xs font-medium block", "{rate.label()}" }
                                                span { class: if active { "text-[10px] opacity-70 block" } else { "text-[10px] text-disabled block" },
                                                    "{rate.description()}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Fee estimate display
                        if let Some(estimated_fee) = fee_estimate() {
                            div { class: "mb-4",
                                p { class: "text-muted text-sm",
                                    "Estimated fee: {format_balance(estimated_fee, unit)}"
                                }
                                if let Some(amount_sats) = parse_dash_amount(&amount_str.read()) {
                                    p { class: "text-foreground font-medium",
                                        "Total: {format_balance(amount_sats.saturating_add(estimated_fee), unit)}"
                                    }
                                }
                            }
                        }

                        // Review button
                        div { class: "flex justify-end",
                            button {
                                class: "bg-dash hover:bg-dash-hover text-foreground text-sm font-medium py-2 px-6 rounded-lg transition-colors",
                                onclick: move |_| {
                                    if validate_form() {
                                        step.set(SendStep::Review);
                                    }
                                },
                                "Review"
                            }
                        }
                    }
                }
            }
        }

        SendStep::Review => {
            let amount_sats = parse_dash_amount(&amount_str.read()).unwrap_or(0);
            let selected_rate = fee_rate();
            let estimated_fee = fee_estimate().unwrap_or(0);
            let total = amount_sats.saturating_add(estimated_fee);
            let remaining = spendable.saturating_sub(total);

            rsx! {
                div { class: "text-foreground p-6",

                    h1 { class: "text-2xl font-bold mb-6", "Review Transaction" }

                    div { class: "bg-card rounded-lg p-6 max-w-xl",

                        div { class: "mb-4",
                            p { class: "text-muted text-sm", "To" }
                            p { class: "font-mono text-sm break-all", "{address}" }
                        }

                        div { class: "mb-4",
                            p { class: "text-muted text-sm", "Amount" }
                            p { class: "text-xl font-bold", "{format_balance(amount_sats, unit)}" }
                        }

                        div { class: "mb-4",
                            p { class: "text-muted text-sm", "Fee Rate" }
                            p { class: "text-foreground",
                                "{selected_rate.label()} ({selected_rate.description()})"
                            }
                        }

                        div { class: "mb-4",
                            p { class: "text-muted text-sm", "Estimated Fee" }
                            p { class: "text-foreground", "{format_balance(estimated_fee, unit)}" }
                        }

                        div { class: "mb-4",
                            p { class: "text-muted text-sm", "Total Deduction" }
                            p { class: "text-foreground font-medium", "{format_balance(total, unit)}" }
                        }

                        div { class: "mb-6",
                            p { class: "text-muted text-sm", "Remaining Balance" }
                            p { class: "text-foreground", "{format_balance(remaining, unit)}" }
                        }

                        div { class: "flex gap-3",

                            button {
                                class: "flex-1 bg-hover hover:bg-edge text-foreground font-medium py-3 px-4 rounded-lg transition-colors",
                                onclick: move |_| {
                                    step.set(SendStep::Form);
                                },
                                "Cancel"
                            }

                            button {
                                class: "flex-1 bg-dash hover:bg-dash-hover text-foreground font-medium py-3 px-4 rounded-lg transition-colors",
                                onclick: move |_| {
                                    step.set(SendStep::Sending);
                                    let addr = address.read().clone();
                                    let rate = selected_rate.sat_per_kb();
                                    spawn(async move {
                                        match backend.read().send(&addr, amount_sats, rate).await {
                                            Ok(txid) => {
                                                step.set(SendStep::Success(hex::encode(txid)));
                                            }
                                            Err(e) => {
                                                step.set(SendStep::Error(format!("{e}")));
                                            }
                                        }
                                    });
                                },
                                "Confirm & Send"
                            }
                        }
                    }
                }
            }
        }

        SendStep::Sending => {
            rsx! {
                div { class: "text-foreground p-6 flex flex-col items-center justify-center min-h-[300px]",
                    div { class: "animate-spin rounded-full h-12 w-12 border-b-2 border-dash mb-4" }
                    p { class: "text-lg", "Broadcasting transaction..." }
                }
            }
        }

        SendStep::Success(txid_hex) => {
            rsx! {
                div { class: "text-foreground p-6",

                    div { class: "bg-card rounded-lg p-6 max-w-xl text-center",

                        p { class: "text-success text-4xl mb-4", "✓" }
                        h2 { class: "text-xl font-bold mb-4", "Transaction sent!" }
                        p { class: "font-mono text-xs text-muted mb-6 break-all select-all",
                            "{txid_hex}"
                        }

                        Link {
                            to: crate::router::Route::Dashboard {},
                            class: "inline-block bg-dash hover:bg-dash-hover text-foreground font-medium py-3 px-6 rounded-lg transition-colors",
                            "Back to Dashboard"
                        }
                    }
                }
            }
        }

        SendStep::Error(msg) => {
            rsx! {
                div { class: "text-foreground p-6",

                    div { class: "bg-card rounded-lg p-6 max-w-xl",

                        h2 { class: "text-xl font-bold mb-4 text-error", "Send Failed" }
                        p { class: "text-foreground mb-6", "{msg}" }

                        button {
                            class: "w-full bg-dash hover:bg-dash-hover text-foreground font-medium py-3 px-4 rounded-lg transition-colors",
                            onclick: move |_| {
                                step.set(SendStep::Form);
                            },
                            "Try Again"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dashcore::PublicKey;

    use super::*;

    fn test_address(network: Network) -> String {
        let pk = PublicKey::from_slice(&[
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x01,
        ])
        .unwrap();
        DashAddress::p2pkh(&pk, network).to_string()
    }

    // -- validate_send_address --

    #[test]
    fn validate_send_address_empty() {
        let err = validate_send_address("", Network::Testnet).unwrap_err();
        assert_eq!(err, "Address is required");
    }

    #[test]
    fn validate_send_address_whitespace_only() {
        let err = validate_send_address("   ", Network::Testnet).unwrap_err();
        assert_eq!(err, "Address is required");
    }

    #[test]
    fn validate_send_address_invalid_format() {
        let err = validate_send_address("not-an-address", Network::Testnet).unwrap_err();
        assert_eq!(err, "Invalid address format");
    }

    #[test]
    fn validate_send_address_wrong_network() {
        let testnet_addr = test_address(Network::Testnet);
        let err = validate_send_address(&testnet_addr, Network::Mainnet).unwrap_err();
        assert_eq!(err, "Address is for a different network");
    }

    #[test]
    fn validate_send_address_valid_testnet() {
        let addr = test_address(Network::Testnet);
        assert!(validate_send_address(&addr, Network::Testnet).is_ok());
    }

    #[test]
    fn validate_send_address_valid_mainnet() {
        let addr = test_address(Network::Mainnet);
        assert!(validate_send_address(&addr, Network::Mainnet).is_ok());
    }

    // -- validate_send_amount --

    #[test]
    fn validate_send_amount_valid() {
        assert_eq!(validate_send_amount("1.5", 200_000_000), Ok(150_000_000));
    }

    #[test]
    fn validate_send_amount_invalid_input() {
        assert_eq!(
            validate_send_amount("abc", 100_000_000),
            Err("Invalid amount".to_string())
        );
    }

    #[test]
    fn validate_send_amount_zero() {
        assert_eq!(
            validate_send_amount("0", 100_000_000),
            Err("Amount must be greater than 0".to_string())
        );
    }

    #[test]
    fn validate_send_amount_exceeds_balance() {
        assert_eq!(
            validate_send_amount("2", 100_000_000),
            Err("Exceeds spendable balance".to_string())
        );
    }

    #[test]
    fn validate_send_amount_exact_balance() {
        assert_eq!(validate_send_amount("1", 100_000_000), Ok(100_000_000));
    }

    #[test]
    fn validate_send_amount_empty() {
        assert_eq!(
            validate_send_amount("", 100_000_000),
            Err("Invalid amount".to_string())
        );
    }

    // -- format_max_amount --

    #[test]
    fn format_max_amount_zero() {
        assert_eq!(format_max_amount(0), "0");
    }

    #[test]
    fn format_max_amount_whole() {
        assert_eq!(format_max_amount(100_000_000), "1");
    }

    #[test]
    fn format_max_amount_with_fraction() {
        assert_eq!(format_max_amount(150_000_000), "1.5");
    }

    #[test]
    fn format_max_amount_one_satoshi() {
        assert_eq!(format_max_amount(1), "0.00000001");
    }

    #[test]
    fn format_max_amount_trailing_zeros_stripped() {
        assert_eq!(format_max_amount(123_400_000), "1.234");
    }

    // -- FeeRate --

    #[test]
    fn fee_rate_labels() {
        assert_eq!(FeeRate::Economy.label(), "Economy");
        assert_eq!(FeeRate::Normal.label(), "Normal");
        assert_eq!(FeeRate::Priority.label(), "Priority");
    }

    #[test]
    fn fee_rate_sat_per_kb() {
        assert_eq!(FeeRate::Economy.sat_per_kb(), 1_000);
        assert_eq!(FeeRate::Normal.sat_per_kb(), 10_000);
        assert_eq!(FeeRate::Priority.sat_per_kb(), 100_000);
    }
}
