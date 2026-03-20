use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::backend::types::TransactionDirection;
use crate::event_bridge::use_event_bridge;
use crate::state::network::NetworkInfo;
use crate::state::view_models::{format_balance, format_transaction};
use crate::state::wallet::WalletState;

#[component]
pub fn Dashboard() -> Element {
    let backend = use_context::<Signal<Backend>>();
    let wallet = use_context::<Signal<WalletState>>();
    let network_info = use_context::<Signal<NetworkInfo>>();

    // Start the event bridge coroutine to pipe backend events into UI state.
    use_event_bridge();

    // Auto-connect: start the backend if it is not already running.
    use_future(move || async move {
        if !backend.read().is_running() {
            let _ = backend.read().start().await;
        }
    });

    let balance = wallet.read().balance;
    let transactions = wallet.read().transactions.clone();
    let current_height = network_info.read().chain_tip;

    rsx! {
        div {
            class: "text-foreground p-6",

            // Balance section
            div {
                class: "mb-8",
                h2 {
                    class: "text-muted text-sm uppercase tracking-wide mb-2",
                    "Available Balance"
                }
                p {
                    class: "text-4xl font-bold",
                    "{format_balance(balance.spendable())}"
                }

                // Breakdown cards for non-zero secondary balances
                div {
                    class: "flex gap-4 mt-4",

                    if balance.unconfirmed() > 0 {
                        BalanceCard { label: "Pending", amount: balance.unconfirmed() }
                    }
                    if balance.immature() > 0 {
                        BalanceCard { label: "Immature", amount: balance.immature() }
                    }
                    if balance.locked() > 0 {
                        BalanceCard { label: "Locked", amount: balance.locked() }
                    }
                }
            }

            // Transaction history
            div {
                h3 {
                    class: "text-lg font-semibold mb-4",
                    "Transactions"
                }

                if transactions.is_empty() {
                    div {
                        class: "text-disabled text-center py-12",
                        p { class: "text-lg", "No transactions yet" }
                    }
                } else {
                    div {
                        class: "space-y-1",
                        for (i, tx) in transactions.iter().enumerate() {
                            {
                                let view = format_transaction(tx, current_height);
                                let is_sent = tx.direction == TransactionDirection::Sent;
                                let bg = if i % 2 == 0 { "bg-card" } else { "bg-surface-alt" };

                                rsx! {
                                    div {
                                        class: "flex items-center justify-between {bg} hover:bg-hover rounded-lg p-4 transition-colors",

                                        // Left: direction + address + time
                                        div {
                                            class: "flex items-center gap-3",
                                            span {
                                                class: if is_sent { "text-error text-lg" } else { "text-success text-lg" },
                                                if is_sent { "▲" } else { "▼" }
                                            }
                                            div {
                                                p {
                                                    class: "font-mono text-sm",
                                                    "{view.address_short}"
                                                }
                                                p {
                                                    class: "text-disabled text-xs",
                                                    "{view.timestamp_display}"
                                                }
                                            }
                                        }

                                        // Right: amount + confirmations + badges
                                        div {
                                            class: "text-right flex items-center gap-2",
                                            div {
                                                p {
                                                    class: if is_sent { "text-error font-medium" } else { "text-success font-medium" },
                                                    "{view.amount_display}"
                                                }
                                                p {
                                                    class: "text-disabled text-xs",
                                                    "{view.confirmations_display}"
                                                }
                                            }
                                            // Badges
                                            if view.is_instant_send {
                                                span {
                                                    class: "bg-dash text-xs rounded-full px-2 py-0.5",
                                                    "IS"
                                                }
                                            }
                                            if view.is_chain_locked {
                                                span {
                                                    class: "bg-purple-600 text-xs rounded-full px-2 py-0.5",
                                                    "CL"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BalanceCard(label: &'static str, amount: u64) -> Element {
    rsx! {
        div {
            class: "bg-card rounded-lg p-4",
            p {
                class: "text-muted text-xs uppercase tracking-wide mb-1",
                "{label}"
            }
            p {
                class: "text-sm font-medium",
                "{format_balance(amount)}"
            }
        }
    }
}
