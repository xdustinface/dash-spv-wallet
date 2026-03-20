use dioxus::prelude::*;

use crate::config::AppConfig;

const NETWORKS: &[(&str, dashcore::Network)] = &[
    ("Mainnet", dashcore::Network::Mainnet),
    ("Testnet", dashcore::Network::Testnet),
    ("Devnet", dashcore::Network::Devnet),
    ("Regtest", dashcore::Network::Regtest),
];

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];

#[component]
pub fn Settings() -> Element {
    let mut config_signal = use_context::<Signal<AppConfig>>();
    let config = config_signal.read().clone();

    let mut save_status = use_signal(|| None::<Result<(), String>>);

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

        match cfg.save() {
            Ok(()) => save_status.set(Some(Ok(()))),
            Err(e) => save_status.set(Some(Err(e.to_string()))),
        }
    };

    rsx! {
        div {
            class: "text-foreground p-6",

            h1 {
                class: "text-2xl font-bold mb-6",
                "Settings"
            }

            div {
                class: "bg-card rounded-lg p-6 max-w-lg space-y-6",

                // Network selector
                div {
                    label {
                        class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Network"
                    }
                    div {
                        class: "flex gap-2",
                        for &(name, net) in NETWORKS {
                            button {
                                class: if network() == net {
                                    "px-4 py-2 rounded-lg bg-dash text-foreground font-medium"
                                } else {
                                    "px-4 py-2 rounded-lg bg-surface-alt text-muted hover:bg-hover transition-colors"
                                },
                                onclick: move |_| network.set(net),
                                "{name}"
                            }
                        }
                    }
                }

                // Data directory
                div {
                    label {
                        class: "block text-muted text-sm uppercase tracking-wide mb-2",
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
                    label {
                        class: "block text-muted text-sm uppercase tracking-wide mb-2",
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
                div {
                    class: "flex items-center justify-between",
                    label {
                        class: "text-muted text-sm uppercase tracking-wide",
                        "Developer Mode"
                    }
                    button {
                        class: if dev_mode() {
                            "w-12 h-6 rounded-full bg-dash transition-colors relative"
                        } else {
                            "w-12 h-6 rounded-full bg-surface-alt transition-colors relative"
                        },
                        onclick: move |_| dev_mode.set(!dev_mode()),
                        div {
                            class: if dev_mode() {
                                "w-5 h-5 rounded-full bg-foreground absolute top-0.5 right-0.5 transition-all"
                            } else {
                                "w-5 h-5 rounded-full bg-muted absolute top-0.5 left-0.5 transition-all"
                            },
                        }
                    }
                }

                // Log level selector
                div {
                    label {
                        class: "block text-muted text-sm uppercase tracking-wide mb-2",
                        "Log Level"
                    }
                    div {
                        class: "flex gap-2",
                        for &level in LOG_LEVELS {
                            button {
                                class: if log_level() == level {
                                    "px-3 py-1 rounded-lg bg-dash text-foreground text-sm font-medium"
                                } else {
                                    "px-3 py-1 rounded-lg bg-surface-alt text-muted text-sm hover:bg-hover transition-colors"
                                },
                                onclick: move |_| log_level.set(level.to_string()),
                                "{level}"
                            }
                        }
                    }
                }

                // Backend selector (dev mode only)
                if dev_mode() {
                    div {
                        label {
                            class: "block text-muted text-sm uppercase tracking-wide mb-2",
                            "Backend"
                        }
                        div {
                            class: "flex gap-2",
                            for &name in backends.iter() {
                                button {
                                    class: if backend() == name {
                                        "px-3 py-1 rounded-lg bg-dash text-foreground text-sm font-medium"
                                    } else {
                                        "px-3 py-1 rounded-lg bg-surface-alt text-muted text-sm hover:bg-hover transition-colors"
                                    },
                                    onclick: move |_| backend.set(name.to_string()),
                                    "{name}"
                                }
                            }
                        }
                        p {
                            class: "text-muted text-xs mt-1",
                            "Requires restart. Use --backend flag at launch."
                        }
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
                        p {
                            class: "text-success text-sm",
                            "Saved. Restart to apply changes."
                        }
                    },
                    Some(Err(e)) => rsx! {
                        p {
                            class: "text-error text-sm",
                            "Failed to save: {e}"
                        }
                    },
                    None => rsx! {},
                }
            }
        }
    }
}
