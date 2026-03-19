use dioxus::prelude::*;

#[component]
pub fn Send() -> Element {
    rsx! {
        div {
            class: "text-white",
            h1 {
                class: "text-2xl font-bold mb-4",
                "Send"
            }
            p {
                class: "text-gray-400",
                "Send form will appear here."
            }
        }
    }
}
