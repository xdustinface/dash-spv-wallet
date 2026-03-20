use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::backend::r#trait::SpvBackend;
use crate::router::Route;
use crate::state::app_state::AppState;

/// BIP-39 test word list subset for generating display mnemonics.
const WORD_LIST: [&str; 64] = [
    "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract",
    "absurd", "abuse", "access", "accident", "account", "accuse", "achieve", "acid",
    "across", "act", "action", "actor", "actress", "actual", "adapt", "add",
    "addict", "address", "adjust", "admit", "adult", "advance", "advice", "afford",
    "agree", "ahead", "aim", "air", "airport", "aisle", "alarm", "album",
    "alert", "alien", "almost", "alone", "alpha", "already", "also", "alter",
    "always", "amateur", "amazing", "among", "amount", "amused", "anchor", "ancient",
    "anger", "angle", "animal", "ankle", "annual", "another", "anxiety", "apart",
];

/// Step in the wallet creation flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Generate,
    DisplayMnemonic,
}

#[component]
pub fn WalletCreate() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let backend = use_context::<Signal<MockBackend>>();
    let navigator = use_navigator();

    let mut step = use_signal(|| Step::Generate);
    let mut mnemonic_words = use_signal(Vec::<String>::new);
    let mut error_message = use_signal(|| None::<String>);
    let mut is_creating = use_signal(|| false);

    let generate_mnemonic = move |_| {
        let words: Vec<String> = (0..12)
            .map(|i| {
                // Simple deterministic-looking selection; real generation will come with NativeBackend.
                let index = (i * 7 + 3) % WORD_LIST.len();
                WORD_LIST[index].to_string()
            })
            .collect();
        mnemonic_words.set(words);
        step.set(Step::DisplayMnemonic);
    };

    let save_and_create = move |_| async move {
        if *is_creating.read() {
            return;
        }
        is_creating.set(true);
        error_message.set(None);

        let phrase = mnemonic_words.read().join(" ");
        match backend.read().create_wallet(&phrase).await {
            Ok(()) => {
                app_state.write().set_wallet_loaded();
                navigator.push(Route::Dashboard {});
            }
            Err(e) => {
                error_message.set(Some(e.to_string()));
                is_creating.set(false);
            }
        }
    };

    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-screen bg-surface text-foreground",

            match *step.read() {
                Step::Generate => rsx! {
                    h1 {
                        class: "text-3xl font-bold mb-2",
                        "Create New Wallet"
                    }
                    p {
                        class: "text-muted mb-10",
                        "Generate a new recovery phrase to create your wallet"
                    }
                    button {
                        class: "px-8 py-3 bg-dash hover:bg-dash-hover rounded-lg text-lg font-semibold transition-colors",
                        onclick: generate_mnemonic,
                        "Generate New Wallet"
                    }
                },
                Step::DisplayMnemonic => rsx! {
                    h1 {
                        class: "text-3xl font-bold mb-2",
                        "Recovery Phrase"
                    }
                    p {
                        class: "text-warning mb-8 max-w-md text-center",
                        "Write down these words in order. You will need them to recover your wallet."
                    }

                    div {
                        class: "grid grid-cols-3 gap-3 mb-8",
                        for (i, word) in mnemonic_words.read().iter().enumerate() {
                            div {
                                class: "flex items-center gap-2 bg-card border border-edge rounded-lg px-4 py-2",
                                span {
                                    class: "text-disabled text-sm w-6 text-right",
                                    "{i + 1}."
                                }
                                span {
                                    class: "text-foreground font-mono",
                                    "{word}"
                                }
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
                        class: "px-8 py-3 bg-success hover:bg-success/80 rounded-lg text-lg font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *is_creating.read(),
                        onclick: save_and_create,
                        if *is_creating.read() {
                            "Creating wallet..."
                        } else {
                            "I've saved my recovery phrase"
                        }
                    }
                },
            }
        }
    }
}
