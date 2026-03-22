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

#[component]
pub fn Send() -> Element {
    let backend = use_context::<Signal<Backend>>();
    let wallet = use_context::<Signal<WalletState>>();
    let config = use_context::<Signal<AppConfig>>();

    let mut address = use_signal(String::new);
    let mut amount_str = use_signal(String::new);
    let mut step = use_signal(|| SendStep::Form);
    let mut address_error = use_signal(|| false);
    let mut amount_error = use_signal(|| None::<String>);
    let mut fee_rate = use_signal(|| FeeRate::Normal);

    let spendable = wallet.read().balance.spendable();
    let unit = config.read().network.currency_unit();

    let mut validate_form = move || -> bool {
        let mut valid = true;

        if address.read().trim().is_empty() {
            address_error.set(true);
            valid = false;
        } else {
            address_error.set(false);
        }

        match parse_dash_amount(&amount_str.read()) {
            None => {
                amount_error.set(Some("Invalid amount".to_string()));
                valid = false;
            }
            Some(0) => {
                amount_error.set(Some("Amount must be greater than 0".to_string()));
                valid = false;
            }
            Some(sats) if sats > spendable => {
                amount_error.set(Some("Exceeds spendable balance".to_string()));
                valid = false;
            }
            Some(_) => {
                amount_error.set(None);
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
                div {
                    class: "text-foreground p-6",

                    h1 {
                        class: "text-2xl font-bold mb-6",
                        "Send"
                    }

                    div {
                        class: "bg-card rounded-lg p-6 max-w-lg",

                        // Address input
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-muted text-sm mb-1",
                                "Destination Address"
                            }
                            input {
                                class: "w-full bg-surface-alt text-foreground rounded-lg p-3 outline-none focus:ring-2 focus:ring-dash",
                                r#type: "text",
                                placeholder: "Dash address",
                                value: "{address}",
                                oninput: move |e| {
                                    address.set(e.value());
                                    address_error.set(false);
                                },
                            }
                            if address_error() {
                                p {
                                    class: "text-error text-sm mt-1",
                                    "Address is required"
                                }
                            }
                        }

                        // Amount input
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-muted text-sm mb-1",
                                "Amount ({unit})"
                            }
                            div {
                                class: "flex gap-2",
                                input {
                                    class: "flex-1 bg-surface-alt text-foreground rounded-lg p-3 outline-none focus:ring-2 focus:ring-dash",
                                    r#type: "text",
                                    placeholder: "0.0",
                                    value: "{amount_str}",
                                    oninput: move |e| {
                                        amount_str.set(e.value());
                                        amount_error.set(None);
                                    },
                                }
                                button {
                                    class: "bg-hover hover:bg-edge text-foreground px-4 rounded-lg transition-colors",
                                    onclick: move |_| {
                                        let whole = spendable / 100_000_000;
                                        let frac = spendable % 100_000_000;
                                        let s = if frac == 0 {
                                            format!("{whole}")
                                        } else {
                                            let frac_str = format!("{frac:08}").trim_end_matches('0').to_string();
                                            format!("{whole}.{frac_str}")
                                        };
                                        amount_str.set(s);
                                        amount_error.set(None);
                                    },
                                    "Max"
                                }
                            }
                            p {
                                class: "text-disabled text-xs mt-1",
                                "Available: {format_balance(spendable, unit)}"
                            }
                            if let Some(err) = amount_error() {
                                p {
                                    class: "text-error text-sm mt-1",
                                    "{err}"
                                }
                            }
                        }

                        // Fee rate selector
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-muted text-sm mb-1",
                                "Fee Rate"
                            }
                            div {
                                class: "flex gap-2",
                                for rate in FeeRate::ALL {
                                    button {
                                        class: if fee_rate() == rate {
                                            "flex-1 px-4 py-2 rounded-lg bg-dash text-foreground font-medium transition-colors"
                                        } else {
                                            "flex-1 px-4 py-2 rounded-lg bg-card text-muted hover:bg-hover transition-colors"
                                        },
                                        onclick: move |_| fee_rate.set(rate),
                                        div {
                                            class: "text-sm font-medium",
                                            "{rate.label()}"
                                        }
                                        div {
                                            class: "text-xs text-muted",
                                            "{rate.description()}"
                                        }
                                    }
                                }
                            }
                        }

                        // Fee estimate display
                        if let Some(estimated_fee) = fee_estimate() {
                            div {
                                class: "mb-4",
                                p {
                                    class: "text-muted text-sm",
                                    "Estimated fee: {format_balance(estimated_fee, unit)}"
                                }
                                if let Some(amount_sats) = parse_dash_amount(&amount_str.read()) {
                                    p {
                                        class: "text-foreground font-medium",
                                        "Total: {format_balance(amount_sats.saturating_add(estimated_fee), unit)}"
                                    }
                                }
                            }
                        }

                        // Review button
                        button {
                            class: "w-full bg-dash hover:bg-dash-hover text-foreground font-medium py-3 px-4 rounded-lg transition-colors",
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

        SendStep::Review => {
            let amount_sats = parse_dash_amount(&amount_str.read()).unwrap_or(0);
            let selected_rate = fee_rate();
            let estimated_fee = fee_estimate().unwrap_or(0);
            let total = amount_sats.saturating_add(estimated_fee);
            let remaining = spendable.saturating_sub(total);

            rsx! {
                div {
                    class: "text-foreground p-6",

                    h1 {
                        class: "text-2xl font-bold mb-6",
                        "Review Transaction"
                    }

                    div {
                        class: "bg-card rounded-lg p-6 max-w-lg",

                        div {
                            class: "mb-4",
                            p {
                                class: "text-muted text-sm",
                                "To"
                            }
                            p {
                                class: "font-mono text-sm break-all",
                                "{address}"
                            }
                        }

                        div {
                            class: "mb-4",
                            p {
                                class: "text-muted text-sm",
                                "Amount"
                            }
                            p {
                                class: "text-xl font-bold",
                                "{format_balance(amount_sats, unit)}"
                            }
                        }

                        div {
                            class: "mb-4",
                            p {
                                class: "text-muted text-sm",
                                "Fee Rate"
                            }
                            p {
                                class: "text-foreground",
                                "{selected_rate.label()} ({selected_rate.description()})"
                            }
                        }

                        div {
                            class: "mb-4",
                            p {
                                class: "text-muted text-sm",
                                "Estimated Fee"
                            }
                            p {
                                class: "text-foreground",
                                "{format_balance(estimated_fee, unit)}"
                            }
                        }

                        div {
                            class: "mb-4",
                            p {
                                class: "text-muted text-sm",
                                "Total Deduction"
                            }
                            p {
                                class: "text-foreground font-medium",
                                "{format_balance(total, unit)}"
                            }
                        }

                        div {
                            class: "mb-6",
                            p {
                                class: "text-muted text-sm",
                                "Remaining Balance"
                            }
                            p {
                                class: "text-foreground",
                                "{format_balance(remaining, unit)}"
                            }
                        }

                        div {
                            class: "flex gap-3",

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
                div {
                    class: "text-foreground p-6 flex flex-col items-center justify-center min-h-[300px]",
                    div {
                        class: "animate-spin rounded-full h-12 w-12 border-b-2 border-dash mb-4",
                    }
                    p {
                        class: "text-lg",
                        "Broadcasting transaction..."
                    }
                }
            }
        }

        SendStep::Success(txid_hex) => {
            let short_txid = if txid_hex.len() > 16 {
                format!("{}...{}", &txid_hex[..8], &txid_hex[txid_hex.len() - 8..])
            } else {
                txid_hex.clone()
            };

            rsx! {
                div {
                    class: "text-foreground p-6",

                    div {
                        class: "bg-card rounded-lg p-6 max-w-lg text-center",

                        p {
                            class: "text-success text-4xl mb-4",
                            "✓"
                        }
                        h2 {
                            class: "text-xl font-bold mb-4",
                            "Transaction sent!"
                        }
                        p {
                            class: "font-mono text-sm text-muted mb-6",
                            title: "{txid_hex}",
                            "{short_txid}"
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
                div {
                    class: "text-foreground p-6",

                    div {
                        class: "bg-card rounded-lg p-6 max-w-lg",

                        h2 {
                            class: "text-xl font-bold mb-4 text-error",
                            "Send Failed"
                        }
                        p {
                            class: "text-foreground mb-6",
                            "{msg}"
                        }

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
