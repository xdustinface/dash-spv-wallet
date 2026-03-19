use dioxus::prelude::*;

#[component]
pub fn Receive() -> Element {
    rsx! {
        div {
            class: "text-white",
            h1 {
                class: "text-2xl font-bold mb-4",
                "Receive"
            }
            p {
                class: "text-gray-400",
                "Receive address will appear here."
            }
        }
    }
}
