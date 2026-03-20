use dioxus::prelude::*;

use crate::backend::types::{TransactionDirection, TransactionInfo};
use crate::config::AppConfig;
use crate::state::network::NetworkInfo;
use crate::state::view_models::{format_transaction, matches_search};
use crate::state::wallet::WalletState;

const PAGE_SIZE: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxFilter {
    All,
    Received,
    Sent,
}

impl TxFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Received => "Received",
            Self::Sent => "Sent",
        }
    }
}

fn apply_filters<'a>(
    txs: &'a [TransactionInfo],
    filter: TxFilter,
    search: &str,
    unit: &str,
) -> Vec<&'a TransactionInfo> {
    txs.iter()
        .filter(|tx| match filter {
            TxFilter::All => true,
            TxFilter::Received => tx.direction == TransactionDirection::Received,
            TxFilter::Sent => tx.direction == TransactionDirection::Sent,
        })
        .filter(|tx| matches_search(tx, search, unit))
        .collect()
}

#[component]
pub fn Transactions() -> Element {
    let wallet = use_context::<Signal<WalletState>>();
    let network_info = use_context::<Signal<NetworkInfo>>();
    let config = use_context::<Signal<AppConfig>>();

    let mut search_query = use_signal(String::new);
    let mut active_filter = use_signal(|| TxFilter::All);
    let mut visible_count = use_signal(|| PAGE_SIZE);

    let transactions = wallet.read().transactions.clone();
    let current_height = network_info.read().chain_tip;
    let unit = config.read().network.currency_unit();

    let filtered = apply_filters(&transactions, *active_filter.read(), &search_query.read(), unit);
    let total_filtered = filtered.len();
    let visible = (*visible_count.read()).min(total_filtered);
    let remaining = total_filtered.saturating_sub(visible);

    rsx! {
        div {
            class: "text-foreground p-6",

            // Header
            div {
                class: "mb-6",
                h2 {
                    class: "text-2xl font-bold mb-1",
                    "Transactions"
                }
                p {
                    class: "text-muted text-sm",
                    "Showing {visible} of {total_filtered} transactions"
                }
            }

            // Search bar
            div {
                class: "relative mb-4",
                input {
                    class: "w-full bg-surface-alt border border-edge rounded-lg p-3 text-foreground placeholder-disabled focus:border-dash focus:outline-none",
                    r#type: "text",
                    placeholder: "Search by txid, address, or amount...",
                    value: "{search_query}",
                    oninput: move |e| {
                        search_query.set(e.value());
                        visible_count.set(PAGE_SIZE);
                    },
                }
                if !search_query.read().is_empty() {
                    button {
                        class: "absolute right-3 top-1/2 -translate-y-1/2 text-muted hover:text-foreground transition-colors",
                        onclick: move |_| {
                            search_query.set(String::new());
                            visible_count.set(PAGE_SIZE);
                        },
                        "✕"
                    }
                }
            }

            // Filter tabs
            div {
                class: "flex gap-2 mb-6",
                for filter in [TxFilter::All, TxFilter::Received, TxFilter::Sent] {
                    {
                        let is_active = *active_filter.read() == filter;
                        let class = if is_active {
                            "px-4 py-2 rounded-lg bg-dash text-foreground font-medium transition-colors"
                        } else {
                            "px-4 py-2 rounded-lg bg-card text-muted hover:bg-hover transition-colors"
                        };
                        rsx! {
                            button {
                                class,
                                onclick: move |_| {
                                    active_filter.set(filter);
                                    visible_count.set(PAGE_SIZE);
                                },
                                "{filter.label()}"
                            }
                        }
                    }
                }
            }

            // Transaction list
            if filtered.is_empty() {
                div {
                    class: "text-disabled text-center py-12",
                    p {
                        class: "text-lg",
                        if transactions.is_empty() {
                            "No transactions yet"
                        } else {
                            "No transactions matching your search"
                        }
                    }
                }
            } else {
                div {
                    class: "space-y-2",
                    for (i, tx) in filtered.iter().take(visible).enumerate() {
                        {
                            let view = format_transaction(tx, current_height, unit);
                            let is_sent = tx.direction == TransactionDirection::Sent;
                            let bg = if i % 2 == 0 { "bg-card" } else { "bg-surface-alt" };
                            let border = if is_sent { "border-l-4 border-error" } else { "border-l-4 border-success" };
                            let address_full = tx.addresses.first().cloned().unwrap_or_default();
                            let confirmations = tx.confirmations(current_height);

                            rsx! {
                                div {
                                    class: "flex items-center justify-between {bg} {border} hover:bg-hover rounded-lg p-4 transition-colors",

                                    // Left: direction + address + time
                                    div {
                                        class: "flex items-center gap-3 min-w-0",
                                        span {
                                            class: if is_sent { "text-error text-lg flex-shrink-0" } else { "text-success text-lg flex-shrink-0" },
                                            if is_sent { "▲" } else { "▼" }
                                        }
                                        div {
                                            class: "min-w-0",
                                            p {
                                                class: "font-mono text-sm truncate",
                                                "{address_full}"
                                            }
                                            p {
                                                class: "text-disabled text-xs",
                                                "{view.timestamp_display}"
                                            }
                                        }
                                    }

                                    // Right: amount + confirmations + height + badges
                                    div {
                                        class: "text-right flex items-center gap-2 flex-shrink-0",
                                        div {
                                            p {
                                                class: if is_sent { "text-error font-medium" } else { "text-success font-medium" },
                                                "{view.amount_display}"
                                            }
                                            p {
                                                class: "text-disabled text-xs",
                                                if confirmations > 0 {
                                                    if let Some(h) = tx.height {
                                                        "{confirmations} confirmations (block {h})"
                                                    } else {
                                                        "{view.confirmations_display}"
                                                    }
                                                } else {
                                                    "Unconfirmed"
                                                }
                                            }
                                        }
                                        if view.is_instant_send {
                                            span {
                                                class: "bg-dash text-foreground text-xs rounded-full px-2 py-0.5",
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
                            }
                        }
                    }
                }

                // Load more / all loaded
                div {
                    class: "mt-4 text-center",
                    if remaining > 0 {
                        button {
                            class: "px-6 py-2 bg-card text-muted hover:bg-hover hover:text-foreground rounded-lg transition-colors",
                            onclick: move |_| {
                                let current = *visible_count.read();
                                visible_count.set(current + PAGE_SIZE);
                            },
                            "Load {PAGE_SIZE.min(remaining)} more ({remaining} remaining)"
                        }
                    } else if total_filtered > PAGE_SIZE {
                        p {
                            class: "text-disabled text-sm py-2",
                            "All transactions loaded"
                        }
                    }
                }
            }
        }
    }
}
