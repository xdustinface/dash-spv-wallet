use dioxus::prelude::*;

use crate::state::app_state::AppState;
use crate::state::dev_log::{DevLog, EventCategory};

#[component]
pub fn DevPanel() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let mut dev_log = use_context::<Signal<DevLog>>();

    let mut is_open = use_signal(|| false);
    let mut filter = use_signal(|| None::<EventCategory>);

    if !app_state.read().dev_mode {
        return rsx! {};
    }

    let entries: Vec<(u64, EventCategory, String)> = {
        let log = dev_log.read();
        log.entries(*filter.read())
            .iter()
            .map(|e| (e.timestamp, e.category, e.message.clone()))
            .collect()
    };

    let entry_count = entries.len();

    rsx! {
        div {
            class: "border-t border-edge bg-surface-alt",

            // Toggle bar
            button {
                class: "flex items-center justify-between w-full px-4 py-1 text-xs text-muted hover:bg-hover transition-colors",
                onclick: move |_| {
                    let current = *is_open.read();
                    is_open.set(!current);
                },

                span {
                    if *is_open.read() {
                        "Dev Log [{entry_count}]"
                    } else {
                        "Dev Log [{entry_count}]"
                    }
                }

                span {
                    class: "text-disabled",
                    if *is_open.read() { "Collapse" } else { "Expand" }
                }
            }

            // Panel content
            if *is_open.read() {
                div {
                    class: "px-4 py-2",

                    // Controls row
                    div {
                        class: "flex items-center gap-2 mb-2",

                        // Filter dropdown
                        FilterButton { label: "All", active: filter.read().is_none(), onclick: move |_| filter.set(None) }
                        FilterButton { label: "Sync", active: *filter.read() == Some(EventCategory::Sync), onclick: move |_| filter.set(Some(EventCategory::Sync)) }
                        FilterButton { label: "Network", active: *filter.read() == Some(EventCategory::Network), onclick: move |_| filter.set(Some(EventCategory::Network)) }
                        FilterButton { label: "Wallet", active: *filter.read() == Some(EventCategory::Wallet), onclick: move |_| filter.set(Some(EventCategory::Wallet)) }
                        FilterButton { label: "Error", active: *filter.read() == Some(EventCategory::Error), onclick: move |_| filter.set(Some(EventCategory::Error)) }

                        // Spacer
                        div { class: "flex-1" }

                        // Clear button
                        button {
                            class: "px-2 py-1 text-xs rounded bg-hover hover:bg-edge text-muted transition-colors",
                            onclick: move |_| dev_log.write().clear(),
                            "Clear"
                        }
                    }

                    // Log entries
                    div {
                        class: "max-h-48 overflow-y-auto font-mono text-xs space-y-0.5",

                        if entries.is_empty() {
                            p {
                                class: "text-disabled py-2",
                                "No events yet."
                            }
                        }

                        for (timestamp, category, message) in entries.iter() {
                            div {
                                class: "flex items-start gap-2 py-0.5",

                                // Timestamp
                                span {
                                    class: "text-disabled shrink-0",
                                    "{format_time(*timestamp)}"
                                }

                                // Category badge
                                span {
                                    class: "shrink-0 px-1.5 py-0.5 rounded text-xs font-medium {category_badge_class(category)}",
                                    "{category_label(category)}"
                                }

                                // Message
                                span {
                                    class: "text-foreground",
                                    "{message}"
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
fn FilterButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if active {
        "px-2 py-1 text-xs rounded bg-dash text-foreground"
    } else {
        "px-2 py-1 text-xs rounded bg-hover text-muted hover:bg-edge transition-colors"
    };

    rsx! {
        button {
            class,
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

fn category_label(category: &EventCategory) -> &'static str {
    match category {
        EventCategory::Sync => "SYNC",
        EventCategory::Network => "NET",
        EventCategory::Wallet => "WALLET",
        EventCategory::Error => "ERR",
    }
}

fn category_badge_class(category: &EventCategory) -> &'static str {
    match category {
        EventCategory::Sync => "bg-dash-dark text-dash",
        EventCategory::Network => "bg-success/20 text-success",
        EventCategory::Wallet => "bg-chainlock text-foreground",
        EventCategory::Error => "bg-error/20 text-error",
    }
}

fn format_time(timestamp: u64) -> String {
    let secs = timestamp % 60;
    let mins = (timestamp / 60) % 60;
    let hours = (timestamp / 3600) % 24;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0), "00:00:00");
    }

    #[test]
    fn format_time_wraps_at_24h() {
        // 25 hours should wrap to 01:00:00
        assert_eq!(format_time(25 * 3600), "01:00:00");
    }

    #[test]
    fn format_time_all_components() {
        // 13h 45m 30s = 13*3600 + 45*60 + 30 = 49530
        assert_eq!(format_time(49530), "13:45:30");
    }

    #[test]
    fn category_labels() {
        assert_eq!(category_label(&EventCategory::Sync), "SYNC");
        assert_eq!(category_label(&EventCategory::Network), "NET");
        assert_eq!(category_label(&EventCategory::Wallet), "WALLET");
        assert_eq!(category_label(&EventCategory::Error), "ERR");
    }

    #[test]
    fn category_badge_classes_are_distinct() {
        let classes: Vec<&str> = [
            EventCategory::Sync,
            EventCategory::Network,
            EventCategory::Wallet,
            EventCategory::Error,
        ]
        .iter()
        .map(|c| category_badge_class(c))
        .collect();

        // Each category should produce a unique CSS class
        for (i, a) in classes.iter().enumerate() {
            for b in &classes[i + 1..] {
                assert_ne!(a, b, "badge classes should be unique per category");
            }
        }
    }

    #[test]
    fn category_badge_class_contains_expected_tokens() {
        assert!(category_badge_class(&EventCategory::Error).contains("error"));
        assert!(category_badge_class(&EventCategory::Network).contains("success"));
        assert!(category_badge_class(&EventCategory::Sync).contains("dash"));
        assert!(category_badge_class(&EventCategory::Wallet).contains("chainlock"));
    }
}
