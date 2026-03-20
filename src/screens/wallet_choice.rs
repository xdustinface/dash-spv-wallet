use dioxus::prelude::*;

use crate::router::Route;

#[component]
pub fn WalletChoice() -> Element {
    let navigator = use_navigator();

    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-screen bg-surface text-foreground",

            h1 {
                class: "text-3xl font-bold mb-2",
                "Wallet Setup"
            }
            p {
                class: "text-muted mb-10",
                "Create a new wallet or import an existing one"
            }

            div {
                class: "flex gap-6",

                div {
                    class: "flex flex-col items-center p-8 bg-card border-2 border-dash rounded-xl cursor-pointer transition-colors hover:bg-dash/10",
                    onclick: move |_| { navigator.push(Route::WalletCreate {}); },

                    div {
                        class: "text-4xl mb-4",
                        "+"
                    }
                    h2 {
                        class: "text-xl font-semibold mb-1",
                        "Create New Wallet"
                    }
                    p {
                        class: "text-sm text-muted",
                        "Generate a new recovery phrase"
                    }
                }

                div {
                    class: "flex flex-col items-center p-8 bg-card border-2 border-success rounded-xl cursor-pointer transition-colors hover:bg-success/10",
                    onclick: move |_| { navigator.push(Route::WalletImport {}); },

                    div {
                        class: "text-4xl mb-4",
                        "↓"  // Using a simple arrow character instead of emoji
                    }
                    h2 {
                        class: "text-xl font-semibold mb-1",
                        "Import Existing Wallet"
                    }
                    p {
                        class: "text-sm text-muted",
                        "Restore from a recovery phrase"
                    }
                }
            }
        }
    }
}
