use dioxus::prelude::*;

#[component]
pub fn WalletImport() -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center min-h-screen bg-gray-900 text-white",
            h1 {
                class: "text-2xl font-bold text-gray-400",
                "Import Wallet"
            }
        }
    }
}
