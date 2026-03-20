use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::backend::r#trait::SpvBackend;
use crate::router::Route;
use crate::state::app_state::AppState;

#[component]
pub fn WalletImport() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let backend = use_context::<Signal<MockBackend>>();
    let navigator = use_navigator();

    let mut mnemonic_input = use_signal(String::new);
    let mut error_message = use_signal(|| None::<String>);
    let mut is_importing = use_signal(|| false);

    let word_count = {
        let input = mnemonic_input.read();
        let trimmed = input.trim();
        if trimmed.is_empty() { 0 } else { trimmed.split_whitespace().count() }
    };

    let is_valid_count = word_count == 12 || word_count == 24;

    let import_wallet = move |_| async move {
        if *is_importing.read() {
            return;
        }
        is_importing.set(true);
        error_message.set(None);

        let phrase = mnemonic_input.read().trim().to_string();
        match backend.read().create_wallet(&phrase).await {
            Ok(()) => {
                app_state.write().set_wallet_loaded();
                navigator.push(Route::Dashboard {});
            }
            Err(e) => {
                error_message.set(Some(e.to_string()));
                is_importing.set(false);
            }
        }
    };

    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-screen bg-surface text-foreground",

            h1 {
                class: "text-3xl font-bold mb-2",
                "Import Wallet"
            }
            p {
                class: "text-muted mb-8",
                "Enter your recovery phrase to restore your wallet"
            }

            div {
                class: "w-full max-w-lg",

                textarea {
                    class: "w-full h-32 p-4 bg-card border border-edge rounded-lg text-foreground font-mono text-sm resize-none focus:outline-none focus:border-dash transition-colors",
                    placeholder: "Enter your 12 or 24 word recovery phrase...",
                    value: "{mnemonic_input}",
                    oninput: move |evt| mnemonic_input.set(evt.value()),
                }

                div {
                    class: "flex justify-between items-center mt-2 mb-6",
                    p {
                        class: if is_valid_count { "text-success text-sm" } else { "text-muted text-sm" },
                        "{word_count} of 12 words"
                    }
                    if word_count > 0 && !is_valid_count {
                        p {
                            class: "text-warning text-sm",
                            "Enter 12 or 24 words"
                        }
                    }
                }

                if let Some(err) = error_message.read().as_ref() {
                    p {
                        class: "text-error mb-4",
                        "{err}"
                    }
                }

                button {
                    class: "w-full px-8 py-3 bg-dash hover:bg-dash-hover rounded-lg text-lg font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: !is_valid_count || *is_importing.read(),
                    onclick: import_wallet,
                    if *is_importing.read() {
                        "Importing..."
                    } else {
                        "Import Wallet"
                    }
                }
            }
        }
    }
}
