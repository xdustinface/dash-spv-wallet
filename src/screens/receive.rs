use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::config::AppConfig;
use crate::state::wallet::WalletState;

#[component]
pub fn Receive() -> Element {
    let backend = use_context::<Signal<Backend>>();
    let wallet = use_context::<Signal<WalletState>>();
    let config = use_context::<Signal<AppConfig>>();

    let mut copied = use_signal(|| false);
    let error_msg = use_signal(|| None::<String>);

    let address = wallet.read().receive_address.clone();
    let unit = config.read().network.currency_unit();

    let generate_address = move |_| {
        let mut wallet = wallet;
        let mut error_msg = error_msg;
        spawn(async move {
            match backend.read().get_receive_address() {
                Ok(addr) => {
                    wallet.write().receive_address = Some(addr);
                    error_msg.set(None);
                }
                Err(e) => {
                    error_msg.set(Some(format!("{e}")));
                }
            }
        });
    };

    let copy_address = move |_| {
        copied.set(true);
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            copied.set(false);
        });
    };

    rsx! {
        div {
            class: "text-foreground p-6",

            h1 {
                class: "text-2xl font-bold mb-6",
                "Receive"
            }

            div {
                class: "max-w-2xl space-y-4",

                p {
                    class: "text-sm text-muted",
                    "Share this address to receive {unit}"
                }

                // Address display
                match &address {
                    Some(addr) => rsx! {
                        div {
                            class: "bg-surface-alt border border-edge rounded-lg p-4",
                            p {
                                class: "font-mono text-sm break-all text-center select-all",
                                "{addr}"
                            }
                        }

                        button {
                            class: "w-full bg-card hover:bg-hover text-foreground font-medium py-3 rounded-lg transition-colors",
                            onclick: copy_address,
                            if copied() { "Copied!" } else { "Copy Address" }
                        }
                    },
                    None => rsx! {
                        div {
                            class: "bg-surface-alt border border-edge rounded-lg p-4 text-center",
                            p {
                                class: "text-disabled",
                                "No address generated yet"
                            }
                        }
                    },
                }

                button {
                    class: "w-full bg-dash hover:bg-dash-hover text-foreground font-semibold py-3 rounded-lg transition-colors",
                    onclick: generate_address,
                    "Generate New Address"
                }

                if let Some(err) = error_msg() {
                    div {
                        class: "bg-error/10 border border-error/30 rounded-lg p-3",
                        p {
                            class: "text-error text-sm",
                            "{err}"
                        }
                    }
                }
            }
        }
    }
}
