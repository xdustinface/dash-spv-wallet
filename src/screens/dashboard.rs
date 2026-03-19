use dioxus::prelude::*;

#[component]
pub fn Dashboard() -> Element {
    rsx! {
        div {
            class: "text-white",
            h1 {
                class: "text-2xl font-bold mb-4",
                "Dashboard"
            }
            p {
                class: "text-gray-400",
                "Wallet overview will appear here."
            }
        }
    }
}
