use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::backend::r#trait::SpvBackend;
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

#[component]
pub fn Send() -> Element {
    let backend = use_context::<Signal<MockBackend>>();
    let wallet = use_context::<Signal<WalletState>>();

    let mut address = use_signal(String::new);
    let mut amount_str = use_signal(String::new);
    let mut step = use_signal(|| SendStep::Form);
    let mut address_error = use_signal(|| false);
    let mut amount_error = use_signal(|| None::<String>);

    let spendable = wallet.read().balance.spendable();

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

    match step() {
        SendStep::Form => {
            rsx! {
                div {
                    class: "text-white p-6",

                    h1 {
                        class: "text-2xl font-bold mb-6",
                        "Send"
                    }

                    div {
                        class: "bg-gray-800 rounded-lg p-6 max-w-lg",

                        // Address input
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-gray-400 text-sm mb-1",
                                "Destination Address"
                            }
                            input {
                                class: "w-full bg-gray-900 text-white rounded-lg p-3 outline-none focus:ring-2 focus:ring-blue-500",
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
                                    class: "text-red-400 text-sm mt-1",
                                    "Address is required"
                                }
                            }
                        }

                        // Amount input
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-gray-400 text-sm mb-1",
                                "Amount (DASH)"
                            }
                            div {
                                class: "flex gap-2",
                                input {
                                    class: "flex-1 bg-gray-900 text-white rounded-lg p-3 outline-none focus:ring-2 focus:ring-blue-500",
                                    r#type: "text",
                                    placeholder: "0.0",
                                    value: "{amount_str}",
                                    oninput: move |e| {
                                        amount_str.set(e.value());
                                        amount_error.set(None);
                                    },
                                }
                                button {
                                    class: "bg-gray-700 hover:bg-gray-600 text-white px-4 rounded-lg transition-colors",
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
                                class: "text-gray-500 text-xs mt-1",
                                "Available: {format_balance(spendable)}"
                            }
                            if let Some(err) = amount_error() {
                                p {
                                    class: "text-red-400 text-sm mt-1",
                                    "{err}"
                                }
                            }
                        }

                        // Review button
                        button {
                            class: "w-full bg-blue-600 hover:bg-blue-500 text-white font-medium py-3 px-4 rounded-lg transition-colors",
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

            rsx! {
                div {
                    class: "text-white p-6",

                    h1 {
                        class: "text-2xl font-bold mb-6",
                        "Review Transaction"
                    }

                    div {
                        class: "bg-gray-800 rounded-lg p-6 max-w-lg",

                        div {
                            class: "mb-4",
                            p {
                                class: "text-gray-400 text-sm",
                                "To"
                            }
                            p {
                                class: "font-mono text-sm break-all",
                                "{address}"
                            }
                        }

                        div {
                            class: "mb-6",
                            p {
                                class: "text-gray-400 text-sm",
                                "Amount"
                            }
                            p {
                                class: "text-xl font-bold",
                                "{format_balance(amount_sats)}"
                            }
                        }

                        div {
                            class: "flex gap-3",

                            button {
                                class: "flex-1 bg-gray-700 hover:bg-gray-600 text-white font-medium py-3 px-4 rounded-lg transition-colors",
                                onclick: move |_| {
                                    step.set(SendStep::Form);
                                },
                                "Cancel"
                            }

                            button {
                                class: "flex-1 bg-blue-600 hover:bg-blue-500 text-white font-medium py-3 px-4 rounded-lg transition-colors",
                                onclick: move |_| {
                                    step.set(SendStep::Sending);
                                    let addr = address.read().clone();
                                    spawn(async move {
                                        match backend.read().send(&addr, amount_sats).await {
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
                    class: "text-white p-6 flex flex-col items-center justify-center min-h-[300px]",
                    div {
                        class: "animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500 mb-4",
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
                    class: "text-white p-6",

                    div {
                        class: "bg-gray-800 rounded-lg p-6 max-w-lg text-center",

                        p {
                            class: "text-green-400 text-4xl mb-4",
                            "✓"
                        }
                        h2 {
                            class: "text-xl font-bold mb-4",
                            "Transaction sent!"
                        }
                        p {
                            class: "font-mono text-sm text-gray-400 mb-6",
                            title: "{txid_hex}",
                            "{short_txid}"
                        }

                        Link {
                            to: crate::router::Route::Dashboard {},
                            class: "inline-block bg-blue-600 hover:bg-blue-500 text-white font-medium py-3 px-6 rounded-lg transition-colors",
                            "Back to Dashboard"
                        }
                    }
                }
            }
        }

        SendStep::Error(msg) => {
            rsx! {
                div {
                    class: "text-white p-6",

                    div {
                        class: "bg-gray-800 rounded-lg p-6 max-w-lg",

                        h2 {
                            class: "text-xl font-bold mb-4 text-red-400",
                            "Send Failed"
                        }
                        p {
                            class: "text-gray-300 mb-6",
                            "{msg}"
                        }

                        button {
                            class: "w-full bg-blue-600 hover:bg-blue-500 text-white font-medium py-3 px-4 rounded-lg transition-colors",
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
