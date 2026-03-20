use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::backend::types::TransactionDirection;
use crate::config::AppConfig;
use crate::event_bridge::use_event_bridge;
use crate::router::Route;
use crate::state::network::NetworkInfo;
use crate::state::view_models::{
    format_address_responsive, format_balance, format_timestamp_absolute, format_transaction,
};
use crate::state::wallet::WalletState;

const DASHBOARD_TX_LIMIT: usize = 5;

#[component]
pub fn Dashboard() -> Element {
    let backend = use_context::<Signal<Backend>>();
    let wallet = use_context::<Signal<WalletState>>();
    let network_info = use_context::<Signal<NetworkInfo>>();
    let config = use_context::<Signal<AppConfig>>();

    // Start the event bridge coroutine to pipe backend events into UI state.
    use_event_bridge();

    // Load persisted wallet and auto-connect if not already running.
    use_future(move || async move {
        let _ = backend.read().load_wallet().await;
        if !backend.read().is_running() {
            let _ = backend.read().start().await;
        }
    });

    // Load persisted transactions and balance on first mount only.
    use_future(move || {
        let mut wallet_state = wallet;
        async move {
            if !wallet_state.read().transactions.is_empty() {
                return;
            }
            if let Ok(txs) = backend.read().get_transactions() {
                wallet_state.write().set_transactions(txs);
            }
            if let Ok(balance) = backend.read().get_balance() {
                wallet_state.write().balance = balance;
            }
        }
    });

    let mut expanded_txid = use_signal(|| None::<dashcore::Txid>);

    let balance = wallet.read().balance;
    let transactions = wallet.read().transactions.clone();
    let current_height = network_info.read().chain_tip;
    let unit = config.read().network.currency_unit();

    rsx! {
        div {
            class: "text-foreground p-6",

            // Balance section
            div {
                class: "mb-6",
                h2 {
                    class: "text-muted text-sm uppercase tracking-wide mb-2",
                    "Available Balance"
                }
                p {
                    class: "text-4xl font-bold",
                    "{format_balance(balance.spendable(), unit)}"
                }

                // Breakdown cards for non-zero secondary balances
                div {
                    class: "flex gap-4 mt-4",

                    if balance.unconfirmed() > 0 {
                        BalanceCard { label: "Pending", amount: balance.unconfirmed(), unit }
                    }
                    if balance.immature() > 0 {
                        BalanceCard { label: "Immature", amount: balance.immature(), unit }
                    }
                    if balance.locked() > 0 {
                        BalanceCard { label: "Locked", amount: balance.locked(), unit }
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
                        for (i, tx) in transactions.iter().take(DASHBOARD_TX_LIMIT).enumerate() {
                            {
                                let view = format_transaction(tx, current_height, unit);
                                let is_sent = tx.direction == TransactionDirection::Sent;
                                let bg = if i % 2 == 0 { "bg-card" } else { "bg-surface-alt" };
                                let border = if is_sent { "border-error" } else { "border-success" };
                                let is_expanded = *expanded_txid.read() == Some(tx.txid);
                                let txid = tx.txid;
                                let address_short = format_address_responsive(&tx.addresses.first().cloned().unwrap_or_default(), 20);
                                let tx = tx.clone();

                                rsx! {
                                    div {
                                        div {
                                            class: "flex items-center justify-between {bg} border-l-4 {border} hover:bg-hover rounded-lg p-3 transition-colors cursor-pointer",
                                            onclick: move |_| {
                                                if *expanded_txid.read() == Some(txid) {
                                                    expanded_txid.set(None);
                                                } else {
                                                    expanded_txid.set(Some(txid));
                                                }
                                            },

                                            // Left: direction + address + time
                                            div {
                                                class: "flex items-center gap-3 min-w-0 flex-1",
                                                span {
                                                    class: if is_sent { "text-error text-lg shrink-0" } else { "text-success text-lg shrink-0" },
                                                    if is_sent { "▲" } else { "▼" }
                                                }
                                                div {
                                                    class: "min-w-0",
                                                    p {
                                                        class: "font-mono text-sm truncate",
                                                        "{address_short}"
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
                                                if view.is_instant_send {
                                                    span {
                                                        class: "bg-dash text-xs rounded-full px-2 py-0.5",
                                                        "IS"
                                                    }
                                                }
                                                if view.is_chain_locked {
                                                    span {
                                                        class: "bg-chainlock text-foreground text-xs rounded-full px-2 py-0.5",
                                                        "CL"
                                                    }
                                                }
                                            }
                                        }

                                        if is_expanded {
                                            div {
                                                class: "bg-surface-alt rounded-b-lg px-4 py-3 -mt-1 mb-1 border-l-4 {border} text-sm space-y-2",

                                                div {
                                                    class: "flex justify-between",
                                                    span { class: "text-muted", "Transaction ID" }
                                                    span { class: "font-mono text-xs select-all", "{tx.txid}" }
                                                }

                                                if let Some(hash) = &tx.block_hash {
                                                    div {
                                                        class: "flex justify-between",
                                                        span { class: "text-muted", "Block Hash" }
                                                        span { class: "font-mono text-xs select-all", "{hash}" }
                                                    }
                                                }

                                                if let Some(h) = tx.height {
                                                    div {
                                                        class: "flex justify-between",
                                                        span { class: "text-muted", "Block Height" }
                                                        span { "{h}" }
                                                    }
                                                }

                                                div {
                                                    class: "flex justify-between",
                                                    span { class: "text-muted", "Date" }
                                                    span { "{format_timestamp_absolute(tx.timestamp)}" }
                                                }

                                                if let Some(fee) = tx.fee {
                                                    div {
                                                        class: "flex justify-between",
                                                        span { class: "text-muted", "Fee" }
                                                        span { "{format_balance(fee, unit)}" }
                                                    }
                                                }

                                                div {
                                                    span { class: "text-muted block mb-1", "Addresses" }
                                                    for addr in &tx.addresses {
                                                        p { class: "font-mono text-xs select-all", "{addr}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if transactions.len() > DASHBOARD_TX_LIMIT {
                        div {
                            class: "mt-4 text-center",
                            Link {
                                to: Route::Transactions {},
                                class: "inline-block w-full px-4 py-2 text-sm text-muted bg-card hover:bg-hover rounded-lg transition-colors",
                                "View all transactions"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BalanceCard(label: &'static str, amount: u64, unit: &'static str) -> Element {
    rsx! {
        div {
            class: "bg-card rounded-lg p-4",
            p {
                class: "text-muted text-xs uppercase tracking-wide mb-1",
                "{label}"
            }
            p {
                class: "text-sm font-medium",
                "{format_balance(amount, unit)}"
            }
        }
    }
}
