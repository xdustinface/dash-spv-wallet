use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::config::AppConfig;

const NETWORKS: &[(&str, dashcore::Network)] = &[
    ("Mainnet", dashcore::Network::Mainnet),
    ("Testnet", dashcore::Network::Testnet),
    ("Devnet", dashcore::Network::Devnet),
    ("Regtest", dashcore::Network::Regtest),
];

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];

const MEMPOOL_STRATEGIES: &[(&str, &str)] =
    &[("Bloom Filter", "bloom-filter"), ("Fetch All", "fetch-all")];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClearCacheState {
    Idle,
    Confirming,
    Clearing,
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{mb:.1} MB");
    }
    let gb = mb / 1024.0;
    format!("{gb:.2} GB")
}

#[component]
pub fn Settings() -> Element {
    let mut config_signal = use_context::<Signal<AppConfig>>();
    let spv_backend = use_context::<Signal<Backend>>();
    let config = config_signal.read().clone();

    let mut save_status = use_signal(|| None::<Result<(), String>>);
    let mut cache_state = use_signal(|| ClearCacheState::Idle);
    let cache_size = use_signal(|| spv_backend.read().cache_size().unwrap_or(0));
    let mut cache_error = use_signal(|| None::<String>);

    let mut data_dir = use_signal(|| config.data_dir.display().to_string());
    let mut wallet_dir = use_signal(|| {
        config
            .wallet_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    });
    let mut network = use_signal(|| config.network);
    let mut dev_mode = use_signal(|| config.dev_mode);
    let mut log_level = use_signal(|| config.log_level.clone());
    let mut mempool_strategy = use_signal(|| config.mempool_strategy().to_string());

    // Reset mempool_strategy when the selected network changes so the UI
    // reflects the correct per-network value.
    use_effect(move || {
        let selected = network();
        let mut cfg = config_signal.read().clone();
        cfg.network = selected;
        mempool_strategy.set(cfg.mempool_strategy().to_string());
    });

    let backends: &[&str] = if cfg!(feature = "ffi") {
        &["native", "ffi"]
    } else {
        &["native"]
    };
    let mut backend = use_signal(|| config.backend.clone());

    let on_save = move |_| {
        let mut cfg = config_signal.write();
        cfg.network = network();
        cfg.data_dir = data_dir().into();
        cfg.wallet_dir = if wallet_dir().is_empty() {
            None
        } else {
            Some(wallet_dir().into())
        };
        cfg.dev_mode = dev_mode();
        cfg.log_level = log_level();
        cfg.network_config_mut().mempool_strategy = mempool_strategy();

        match cfg.save(&AppConfig::default_config_path()) {
            Ok(()) => save_status.set(Some(Ok(()))),
            Err(e) => save_status.set(Some(Err(e.to_string()))),
        }
    };

    rsx! {
        div { class: "text-foreground p-6",

            h1 { class: "text-2xl font-bold mb-6", "Settings" }

            div { class: "bg-card rounded-lg p-6 max-w-lg space-y-6",

                // Network selector
                div {
                    label { class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Network"
                    }
                    div { class: "flex gap-2",
                        for & (name , net) in NETWORKS {
                            button {
                                class: if network() == net { "px-4 py-2 rounded-lg bg-dash text-foreground font-medium" } else { "px-4 py-2 rounded-lg bg-surface-alt text-muted hover:bg-hover transition-colors" },
                                onclick: move |_| network.set(net),
                                "{name}"
                            }
                        }
                    }
                }

                // Data directory
                div {
                    label { class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Data Directory"
                    }
                    input {
                        class: "w-full bg-surface-alt text-foreground rounded-lg px-4 py-2 font-mono text-sm",
                        r#type: "text",
                        value: "{data_dir}",
                        oninput: move |evt| data_dir.set(evt.value()),
                    }
                }

                // Wallet directory
                div {
                    label { class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Wallet Directory"
                    }
                    input {
                        class: "w-full bg-surface-alt text-foreground rounded-lg px-4 py-2 font-mono text-sm",
                        r#type: "text",
                        placeholder: "(default: <data_dir>/wallets/)",
                        value: "{wallet_dir}",
                        oninput: move |evt| wallet_dir.set(evt.value()),
                    }
                }

                // Dev mode toggle
                div { class: "flex items-center justify-between",
                    label { class: "text-muted text-sm uppercase tracking-wide", "Developer Mode" }
                    button {
                        class: if dev_mode() { "w-12 h-6 rounded-full bg-dash transition-colors relative" } else { "w-12 h-6 rounded-full bg-surface-alt transition-colors relative" },
                        onclick: move |_| dev_mode.set(!dev_mode()),
                        div { class: if dev_mode() { "w-5 h-5 rounded-full bg-foreground absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 rounded-full bg-muted absolute top-0.5 left-0.5 transition-all" } }
                    }
                }

                // Log level selector
                div {
                    label { class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Log Level"
                    }
                    div { class: "flex gap-2",
                        for & level in LOG_LEVELS {
                            button {
                                class: if log_level() == level { "px-3 py-1 rounded-lg bg-dash text-foreground text-sm font-medium" } else { "px-3 py-1 rounded-lg bg-surface-alt text-muted text-sm hover:bg-hover transition-colors" },
                                onclick: move |_| log_level.set(level.to_string()),
                                "{level}"
                            }
                        }
                    }
                }

                // Mempool strategy selector
                div {
                    label { class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Mempool Strategy"
                    }
                    div { class: "flex gap-2",
                        for & (label , value) in MEMPOOL_STRATEGIES {
                            button {
                                class: if mempool_strategy() == value { "px-3 py-1 rounded-lg bg-dash text-foreground text-sm font-medium" } else { "px-3 py-1 rounded-lg bg-surface-alt text-muted text-sm hover:bg-hover transition-colors" },
                                onclick: move |_| mempool_strategy.set(value.to_string()),
                                "{label}"
                            }
                        }
                    }
                    p { class: "text-muted text-xs mt-1", "Requires restart." }
                }

                // Backend selector (dev mode only)
                if dev_mode() {
                    div {
                        label { class: "block text-muted text-sm uppercase tracking-wide mb-2",
                            "Backend"
                        }
                        div { class: "flex gap-2",
                            for & name in backends.iter() {
                                button {
                                    class: if backend() == name { "px-3 py-1 rounded-lg bg-dash text-foreground text-sm font-medium" } else { "px-3 py-1 rounded-lg bg-surface-alt text-muted text-sm hover:bg-hover transition-colors" },
                                    onclick: move |_| backend.set(name.to_string()),
                                    "{name}"
                                }
                            }
                        }
                        p { class: "text-muted text-xs mt-1",
                            "Requires restart. Use --backend flag at launch."
                        }
                    }
                }

                // Clear Cache
                div {
                    label { class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Cache"
                    }
                    p { class: "text-foreground text-sm mb-2",
                        "SPV data size: {format_bytes(cache_size())}"
                    }
                    match cache_state() {
                        ClearCacheState::Idle => rsx! {
                            button {
                                class: "w-full bg-error hover:opacity-80 text-foreground font-medium py-2 px-4 rounded-lg transition-colors",
                                onclick: move |_| cache_state.set(ClearCacheState::Confirming),
                                "Clear Cache"
                            }
                        },
                        ClearCacheState::Confirming => rsx! {
                            p { class: "text-error text-sm mb-2",
                                "This will shut down the wallet and clear all sync data. The app will close and re-sync from scratch on next launch. Your wallet and settings will be preserved."
                            }
                            div { class: "flex gap-2",
                                button {
                                    class: "flex-1 bg-error hover:opacity-80 text-foreground font-medium py-2 px-4 rounded-lg transition-colors",
                                    onclick: move |_| async move {
                                        cache_state.set(ClearCacheState::Clearing);
                                        cache_error.set(None);
                                        match spv_backend.read().clear_cache().await {
                                            Ok(()) => {
                                                std::process::exit(0);
                                            }
                                            Err(e) => {
                                                cache_error.set(Some(e.to_string()));
                                                cache_state.set(ClearCacheState::Idle);
                                            }
                                        }
                                    },
                                    "Confirm"
                                }
                                button {
                                    class: "flex-1 bg-surface-alt hover:bg-hover text-muted font-medium py-2 px-4 rounded-lg transition-colors",
                                    onclick: move |_| cache_state.set(ClearCacheState::Idle),
                                    "Cancel"
                                }
                            }
                        },
                        ClearCacheState::Clearing => rsx! {
                            p { class: "text-muted text-sm", "Clearing cache..." }
                        },
                    }
                    if let Some(err) = cache_error() {
                        p { class: "text-error text-sm mt-1", "Failed to clear cache: {err}" }
                    }
                }

                // Save button
                button {
                    class: "w-full bg-dash hover:bg-dash-hover text-foreground font-medium py-2 px-4 rounded-lg transition-colors",
                    onclick: on_save,
                    "Save"
                }

                // Status message
                match save_status() {
                    Some(Ok(())) => rsx! {
                        p { class: "text-success text-sm", "Saved. Restart to apply changes." }
                    },
                    Some(Err(e)) => rsx! {
                        p { class: "text-error text-sm", "Failed to save: {e}" }
                    },
                    None => rsx! {},
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_thresholds() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 500), "500.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(
            format_bytes(1024 * 1024 * 1024 * 2 + 1024 * 1024 * 512),
            "2.50 GB"
        );
    }
}
