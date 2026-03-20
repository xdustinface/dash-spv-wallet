use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::backend::r#trait::SpvBackend;
use crate::state::wallet::WalletState;

#[component]
pub fn Receive() -> Element {
    let backend = use_context::<Signal<MockBackend>>();
    let wallet = use_context::<Signal<WalletState>>();

    let mut copied = use_signal(|| false);
    let error_msg = use_signal(|| None::<String>);

    let address = wallet.read().receive_address.clone();

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
        // Reset after a brief visual feedback period
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
                class: "bg-card rounded-lg p-6 max-w-lg",

                p {
                    class: "text-muted text-sm mb-4",
                    "Share this address to receive DASH"
                }

                // Address display
                match &address {
                    Some(addr) => rsx! {
                        div {
                            class: "bg-surface-alt rounded-lg p-4 mb-4",
                            p {
                                class: "font-mono text-sm break-all text-center select-all",
                                "{addr}"
                            }
                        }

                        button {
                            class: "w-full bg-hover hover:bg-edge text-foreground font-medium py-2 px-4 rounded-lg transition-colors mb-3",
                            onclick: copy_address,
                            if copied() { "Copied!" } else { "Copy Address" }
                        }
                    },
                    None => rsx! {
                        div {
                            class: "bg-surface-alt rounded-lg p-4 mb-4 text-center",
                            p {
                                class: "text-disabled",
                                "No address generated yet"
                            }
                        }
                    },
                }

                button {
                    class: "w-full bg-dash hover:bg-dash-hover text-foreground font-medium py-2 px-4 rounded-lg transition-colors",
                    onclick: generate_address,
                    "Generate New Address"
                }

                if let Some(err) = error_msg() {
                    p {
                        class: "text-error text-sm mt-3",
                        "{err}"
                    }
                }
            }
        }
    }
}
