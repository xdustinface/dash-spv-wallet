use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::router::Route;
use crate::state::app_state::AppState;

/// Count the number of whitespace-separated words in a mnemonic input.
fn mnemonic_word_count(input: &str) -> usize {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        0
    } else {
        trimmed.split_whitespace().count()
    }
}

/// Check whether a word count is valid for a BIP-39 mnemonic (12 or 24 words).
fn is_valid_mnemonic_word_count(count: usize) -> bool {
    count == 12 || count == 24
}

#[component]
pub fn WalletImport() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let backend = use_context::<Signal<Backend>>();
    let navigator = use_navigator();

    let mut mnemonic_input = use_signal(String::new);
    let mut error_message = use_signal(|| None::<String>);
    let mut is_importing = use_signal(|| false);

    let word_count = mnemonic_word_count(&mnemonic_input.read());
    let is_valid_count = is_valid_mnemonic_word_count(word_count);

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
        div { class: "flex flex-col items-center justify-center min-h-screen bg-surface text-foreground",

            h1 { class: "text-3xl font-bold mb-2", "Import Wallet" }
            p { class: "text-muted mb-8", "Enter your recovery phrase to restore your wallet" }

            div { class: "w-full max-w-lg",

                textarea {
                    class: "w-full h-32 p-4 bg-card border border-edge rounded-lg text-foreground font-mono text-sm resize-none focus:outline-none focus:border-dash transition-colors",
                    placeholder: "Enter your 12 or 24 word recovery phrase...",
                    value: "{mnemonic_input}",
                    oninput: move |evt| mnemonic_input.set(evt.value()),
                }

                div { class: "flex justify-between items-center mt-2 mb-6",
                    p { class: if is_valid_count { "text-success text-sm" } else { "text-muted text-sm" },
                        "{word_count} of 12 words"
                    }
                    if word_count > 0 && !is_valid_count {
                        p { class: "text-warning text-sm", "Enter 12 or 24 words" }
                    }
                }

                if let Some(err) = error_message.read().as_ref() {
                    p { class: "text-error mb-4", "{err}" }
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

#[cfg(test)]
mod tests {
    use super::*;

    // -- mnemonic_word_count --

    #[test]
    fn mnemonic_word_count_empty() {
        assert_eq!(mnemonic_word_count(""), 0);
    }

    #[test]
    fn mnemonic_word_count_whitespace_only() {
        assert_eq!(mnemonic_word_count("   \t  \n  "), 0);
    }

    #[test]
    fn mnemonic_word_count_twelve_words() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        assert_eq!(mnemonic_word_count(phrase), 12);
    }

    #[test]
    fn mnemonic_word_count_extra_spaces() {
        let phrase = "  word1  word2   word3  ";
        assert_eq!(mnemonic_word_count(phrase), 3);
    }

    #[test]
    fn mnemonic_word_count_single_word() {
        assert_eq!(mnemonic_word_count("abandon"), 1);
    }

    // -- is_valid_mnemonic_word_count --

    #[test]
    fn valid_mnemonic_word_counts() {
        assert!(is_valid_mnemonic_word_count(12));
        assert!(is_valid_mnemonic_word_count(24));
    }

    #[test]
    fn invalid_mnemonic_word_counts() {
        assert!(!is_valid_mnemonic_word_count(0));
        assert!(!is_valid_mnemonic_word_count(1));
        assert!(!is_valid_mnemonic_word_count(11));
        assert!(!is_valid_mnemonic_word_count(13));
        assert!(!is_valid_mnemonic_word_count(23));
        assert!(!is_valid_mnemonic_word_count(25));
    }
}
