use dioxus::prelude::*;

use crate::backend::types::{InputInfo, OutputInfo, TransactionDirection, TransactionInfo};
use crate::config::AppConfig;
use crate::state::network::NetworkInfo;
use crate::state::view_models::{
    COLLAPSE_THRESHOLD, TransactionView, format_address_responsive, format_transaction,
    matches_search, visible_input_views, visible_output_views,
};
use crate::state::wallet::WalletState;

const PAGE_SIZE: usize = 50;

/// Filter tabs: `None` means "All", `Some(dir)` filters to that direction.
const FILTER_TABS: [Option<TransactionDirection>; 5] = [
    None,
    Some(TransactionDirection::Incoming),
    Some(TransactionDirection::Outgoing),
    Some(TransactionDirection::Internal),
    Some(TransactionDirection::CoinJoin),
];

fn filter_label(filter: Option<TransactionDirection>) -> &'static str {
    match filter {
        None => "All",
        Some(TransactionDirection::Incoming) => "Received",
        Some(TransactionDirection::Outgoing) => "Sent",
        Some(TransactionDirection::Internal) => "Internal",
        Some(TransactionDirection::CoinJoin) => "CoinJoin",
    }
}

fn apply_filters<'a>(
    txs: &'a [TransactionInfo],
    filter: Option<TransactionDirection>,
    search: &str,
    unit: &str,
) -> Vec<&'a TransactionInfo> {
    txs.iter()
        .filter(|tx| match filter {
            None => true,
            Some(dir) => tx.direction == dir,
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
    let mut active_filter = use_signal(|| None::<TransactionDirection>);
    let mut visible_count = use_signal(|| PAGE_SIZE);
    let mut expanded_txid = use_signal(|| None::<dashcore::Txid>);

    let transactions = wallet.read().transactions.clone();
    let current_height = network_info.read().chain_tip;
    let unit = config.read().network.currency_unit();

    let filtered = apply_filters(
        &transactions,
        *active_filter.read(),
        &search_query.read(),
        unit,
    );
    let total_filtered = filtered.len();
    let visible = (*visible_count.read()).min(total_filtered);
    let remaining = total_filtered.saturating_sub(visible);

    rsx! {
        div { class: "text-foreground p-6",

            // Header
            div { class: "mb-6",
                h2 { class: "text-2xl font-bold mb-1", "Transactions" }
                p { class: "text-muted text-sm", "Showing {visible} of {total_filtered} transactions" }
            }

            // Search bar
            div { class: "relative mb-4",
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
            div { class: "flex gap-2 mb-6",
                for filter in FILTER_TABS {
                    {
                        let is_active = *active_filter.read() == filter;
                        let class = if is_active {
                            "px-4 py-2 rounded-lg bg-dash text-foreground font-medium transition-colors"
                        } else {
                            "px-4 py-2 rounded-lg bg-card text-muted hover:bg-hover transition-colors"
                        };
                        let label = filter_label(filter);
                        rsx! {
                            button {
                                class,
                                onclick: move |_| {
                                    active_filter.set(filter);
                                    visible_count.set(PAGE_SIZE);
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }

            // Transaction list
            if filtered.is_empty() {
                div { class: "text-disabled text-center py-12",
                    p { class: "text-lg",
                        if transactions.is_empty() {
                            "No transactions yet"
                        } else {
                            "No transactions matching your search"
                        }
                    }
                }
            } else {
                div { class: "space-y-2",
                    for (i , tx) in filtered.iter().take(visible).enumerate() {
                        {
                            let view = format_transaction(tx, current_height, unit);
                            let bg = if i % 2 == 0 { "bg-card" } else { "bg-surface-alt" };
                            let address_short = format_address_responsive(
                                &tx.addresses.first().cloned().unwrap_or_default(),
                                30,
                            );
                            let confirmations = tx.confirmations(current_height);
                            let is_expanded = *expanded_txid.read() == Some(tx.txid);
                            let txid = tx.txid;
                            let tx = (*tx).clone();
                            rsx! {
                                TransactionRow {
                                    view,
                                    bg,
                                    address_short,
                                    confirmations,
                                    height: tx.height,
                                    is_expanded,
                                    addresses: tx.addresses.clone(),
                                    inputs: tx.inputs.clone(),
                                    outputs: tx.outputs.clone(),
                                    unit: unit.to_string(),
                                    onclick: move |_| {
                                        if *expanded_txid.read() == Some(txid) {
                                            expanded_txid.set(None);
                                        } else {
                                            expanded_txid.set(Some(txid));
                                        }
                                    },
                                }
                            }
                        }
                    }
                }

                // Load more / all loaded
                div { class: "mt-4 text-center",
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
                        p { class: "text-disabled text-sm py-2", "All transactions loaded" }
                    }
                }
            }
        }
    }
}

#[component]
fn TransactionRow(
    view: TransactionView,
    bg: &'static str,
    address_short: String,
    confirmations: u32,
    height: Option<u32>,
    is_expanded: bool,
    addresses: Vec<String>,
    inputs: Vec<InputInfo>,
    outputs: Vec<OutputInfo>,
    unit: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let mut inputs_expanded = use_signal(|| false);
    let mut outputs_expanded = use_signal(|| false);

    let border = view.border_class;
    let icon_class = view.direction_icon_class;
    let icon = view.direction_icon;
    let amount_class = view.amount_class;

    rsx! {
        div {
            div {
                class: "flex items-center justify-between {bg} border-l-4 {border} hover:bg-hover rounded-lg p-3 transition-colors cursor-pointer",
                onclick: move |e| onclick.call(e),

                div { class: "flex items-center gap-3 min-w-0",
                    span { class: icon_class, "{icon}" }
                    div { class: "min-w-0",
                        p { class: "font-mono text-sm truncate", "{address_short}" }
                        p { class: "text-disabled text-xs", "{view.timestamp_display}" }
                    }
                }

                div { class: "text-right flex items-center gap-2 flex-shrink-0",
                    div {
                        p { class: amount_class, "{view.amount_display}" }
                        p { class: "text-disabled text-xs",
                            if confirmations > 0 {
                                if let Some(h) = height {
                                    "{confirmations} confirmations (block {h})"
                                } else {
                                    "{view.confirmations_display}"
                                }
                            } else {
                                "Unconfirmed"
                            }
                        }
                    }
                    if let Some(badge) = view.type_badge {
                        span { class: "bg-surface-alt text-muted text-xs rounded-full px-2 py-0.5 border border-edge",
                            "{badge}"
                        }
                    }
                    if view.is_instant_send {
                        span { class: "bg-dash text-foreground text-xs rounded-full px-2 py-0.5",
                            "IS"
                        }
                    }
                    if view.is_chain_locked {
                        span { class: "bg-chainlock text-foreground text-xs rounded-full px-2 py-0.5",
                            "CL"
                        }
                    }
                }
            }

            if is_expanded {
                div { class: "bg-surface-alt rounded-b-lg px-4 py-3 -mt-1 mb-1 border-l-4 {border} text-sm space-y-2",

                    div { class: "flex justify-between",
                        span { class: "text-muted", "Transaction ID" }
                        span { class: "font-mono text-xs select-all", "{view.txid_hex}" }
                    }

                    if let Some(hash) = &view.block_hash_display {
                        div { class: "flex justify-between",
                            span { class: "text-muted", "Block Hash" }
                            span { class: "font-mono text-xs select-all", "{hash}" }
                        }
                    }

                    if let Some(h) = &view.height_display {
                        div { class: "flex justify-between",
                            span { class: "text-muted", "Block Height" }
                            span { "{h}" }
                        }
                    }

                    div { class: "flex justify-between",
                        span { class: "text-muted", "Date" }
                        span { "{view.absolute_date}" }
                    }

                    if let Some(fee) = &view.fee_display {
                        div { class: "flex justify-between",
                            span { class: "text-muted", "Fee" }
                            span { "{fee}" }
                        }
                    }

                    div {
                        span { class: "text-muted block mb-1", "Addresses" }
                        for addr in &addresses {
                            p { class: "font-mono text-xs select-all", "{addr}" }
                        }
                    }

                    if !inputs.is_empty() {
                        {
                            let input_count = inputs.len();
                            let (input_views, has_more_inputs) = visible_input_views(
                                &inputs,
                                *inputs_expanded.read(),
                                COLLAPSE_THRESHOLD,
                                &unit,
                            );
                            let show_all_inputs = *inputs_expanded.read() || !has_more_inputs;
                            rsx! {
                                div { class: "mt-2 pt-2 border-t border-edge",
                                    span { class: "text-muted font-medium block mb-1", "Inputs ({input_count})" }
                                    div { class: "space-y-1",
                                        for iv in &input_views {
                                            div { class: "flex items-center gap-2 text-xs",
                                                span { class: "text-disabled w-6", "#{iv.index}" }
                                                span { class: "font-mono truncate flex-1", "{iv.address_short}" }
                                                span { class: "text-foreground font-medium", "{iv.amount_display}" }
                                            }
                                        }
                                    }
                                    if has_more_inputs {
                                        button {
                                            class: "text-dash text-xs mt-1 hover:underline",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                let current = *inputs_expanded.read();
                                                inputs_expanded.set(!current);
                                            },
                                            if show_all_inputs {
                                                "Show less"
                                            } else {
                                                "Show all {input_count} inputs"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !outputs.is_empty() {
                        {
                            let output_count = outputs.len();
                            let (output_views, has_more_outputs) = visible_output_views(
                                &outputs,
                                *outputs_expanded.read(),
                                COLLAPSE_THRESHOLD,
                                &unit,
                            );
                            let show_all_outputs = *outputs_expanded.read() || !has_more_outputs;
                            rsx! {
                                div { class: "mt-2 pt-2 border-t border-edge",
                                    span { class: "text-muted font-medium block mb-1", "Outputs ({output_count})" }
                                    div { class: "space-y-1",
                                        for ov in &output_views {
                                            div { class: "flex items-center gap-2 text-xs",
                                                span { class: "text-disabled w-6", "#{ov.index}" }
                                                span { class: "font-mono truncate flex-1", "{ov.address_short}" }
                                                span { class: "text-foreground font-medium", "{ov.amount_display}" }
                                                span { class: "{ov.role_color} text-foreground rounded-full px-2 py-0.5 text-xs",
                                                    "{ov.role_label}"
                                                }
                                            }
                                        }
                                    }
                                    if has_more_outputs {
                                        button {
                                            class: "text-dash text-xs mt-1 hover:underline",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                let current = *outputs_expanded.read();
                                                outputs_expanded.set(!current);
                                            },
                                            if show_all_outputs {
                                                "Show less"
                                            } else {
                                                "Show all {output_count} outputs"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "mt-2 pt-2 border-t border-edge text-center",
                        span { class: "text-muted text-xs", "View full details" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dashcore::hashes::Hash;

    use crate::backend::types::TransactionType;

    use super::*;

    fn sample_txs() -> Vec<TransactionInfo> {
        vec![
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([0xAA; 32]),
                amount: 100_000_000,
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 1700000000,
                height: Some(100),
                fee: None,
                addresses: vec!["yAddr1".into()],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([0xBB; 32]),
                amount: -50_000_000,
                direction: TransactionDirection::Outgoing,
                transaction_type: TransactionType::Standard,
                timestamp: 1700001000,
                height: Some(101),
                fee: Some(226),
                addresses: vec!["yAddr2".into()],
                block_hash: None,
                is_instant_send: false,
                is_chain_locked: false,
                label: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
            },
            TransactionInfo {
                txid: dashcore::Txid::from_byte_array([0xCC; 32]),
                amount: 200_000_000,
                direction: TransactionDirection::Incoming,
                transaction_type: TransactionType::Standard,
                timestamp: 1700002000,
                height: Some(102),
                fee: None,
                addresses: vec!["yAddr3".into()],
                block_hash: None,
                is_instant_send: true,
                is_chain_locked: false,
                label: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
            },
        ]
    }

    #[test]
    fn apply_filters_all() {
        let txs = sample_txs();
        let result = apply_filters(&txs, None, "", "DASH");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn apply_filters_received_only() {
        let txs = sample_txs();
        let result = apply_filters(&txs, Some(TransactionDirection::Incoming), "", "DASH");
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|tx| tx.direction == TransactionDirection::Incoming)
        );
    }

    #[test]
    fn apply_filters_sent_only() {
        let txs = sample_txs();
        let result = apply_filters(&txs, Some(TransactionDirection::Outgoing), "", "DASH");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].direction, TransactionDirection::Outgoing);
    }

    #[test]
    fn apply_filters_with_search() {
        let txs = sample_txs();
        let result = apply_filters(&txs, None, "yAddr2", "DASH");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].addresses[0], "yAddr2");
    }

    #[test]
    fn apply_filters_no_match() {
        let txs = sample_txs();
        let result = apply_filters(&txs, None, "zzz_nonexistent", "DASH");
        assert!(result.is_empty());
    }

    #[test]
    fn apply_filters_combined_filter_and_search() {
        let txs = sample_txs();
        let result = apply_filters(&txs, Some(TransactionDirection::Incoming), "yAddr3", "DASH");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].addresses[0], "yAddr3");
    }

    #[test]
    fn apply_filters_empty_transactions() {
        let txs: Vec<TransactionInfo> = vec![];
        let result = apply_filters(&txs, None, "", "DASH");
        assert!(result.is_empty());
    }

    #[test]
    fn apply_filters_internal_only() {
        let mut txs = sample_txs();
        txs.push(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xDD; 32]),
            amount: 10_000,
            direction: TransactionDirection::Internal,
            transaction_type: TransactionType::Standard,
            timestamp: 1700003000,
            height: Some(103),
            fee: None,
            addresses: vec!["yAddr4".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        });

        let result = apply_filters(&txs, Some(TransactionDirection::Internal), "", "DASH");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].direction, TransactionDirection::Internal);
    }

    #[test]
    fn apply_filters_coinjoin_only() {
        let mut txs = sample_txs();
        txs.push(TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xEE; 32]),
            amount: -25_000_000,
            direction: TransactionDirection::CoinJoin,
            transaction_type: TransactionType::CoinJoin,
            timestamp: 1700004000,
            height: Some(104),
            fee: Some(100),
            addresses: vec!["yAddr5".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        });

        let result = apply_filters(&txs, Some(TransactionDirection::CoinJoin), "", "DASH");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].direction, TransactionDirection::CoinJoin);
    }

    #[test]
    fn filter_labels() {
        assert_eq!(filter_label(None), "All");
        assert_eq!(
            filter_label(Some(TransactionDirection::Incoming)),
            "Received"
        );
        assert_eq!(filter_label(Some(TransactionDirection::Outgoing)), "Sent");
        assert_eq!(
            filter_label(Some(TransactionDirection::Internal)),
            "Internal"
        );
        assert_eq!(
            filter_label(Some(TransactionDirection::CoinJoin)),
            "CoinJoin"
        );
    }
}
